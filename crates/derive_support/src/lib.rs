//! Shared syn helpers for the laserbeam and bind derives. Child-path construction is shared so `resolve` and `dispatch` descend identically.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::spanned::Spanned;
use syn::{Fields, Ident, Index, Member, Path, Type};

/// Route enum (`parent`) and its `Above::Up` half (`up`) for a multi-parent child. The derive can build neither; a route without its Up half has no `Above` impl.
pub struct Route {
    pub parent: Path,
    pub up: Path,
}

pub type Child = (Member, Type, Option<Route>);

/// `#[child]` fields of a struct, in declaration order.
///
/// # Errors
///
/// Errors if a field's route attribute is malformed.
pub fn find_children(fields: &Fields) -> syn::Result<Vec<Child>> {
    let mut found = Vec::new();
    for (i, f) in fields.iter().enumerate() {
        if !f.attrs.iter().any(|a| a.path().is_ident("child")) {
            continue;
        }
        let member = f
            .ident
            .clone()
            .map_or_else(|| Member::Unnamed(Index::from(i)), Member::Named);
        found.push((member, f.ty.clone(), parent_route(&f.attrs)?));
    }
    Ok(found)
}

/// Route from `#[child(route = Enum, up = UpEnum)]`. A bare `#[child]` is a single-parent child.
///
/// # Errors
///
/// Errors if the attribute list contains anything other than `route = ..` and `up = ..`, or if one of the two is given without the other.
pub fn parent_route(attrs: &[syn::Attribute]) -> syn::Result<Option<Route>> {
    let Some(attr) = attrs.iter().find(|a| a.path().is_ident("child")) else {
        return Ok(None);
    };
    let mut parent = None;
    let mut up = None;
    if matches!(attr.meta, syn::Meta::List(_)) {
        attr.parse_nested_meta(|m| {
            if m.path.is_ident("route") {
                parent = Some(m.value()?.parse()?);
                Ok(())
            } else if m.path.is_ident("up") {
                up = Some(m.value()?.parse()?);
                Ok(())
            } else {
                Err(m.error("expected `route` or `up`"))
            }
        })?;
    }
    match (parent, up) {
        (None, None) => Ok(None),
        (Some(parent), Some(up)) => Ok(Some(Route { parent, up })),
        (Some(_), None) => Err(syn::Error::new(
            attr.span(),
            "`#[child(route = ..)]` needs `up = ..`, the route enum's `Above::Up` half",
        )),
        (None, Some(_)) => Err(syn::Error::new(
            attr.span(),
            "`#[child(up = ..)]` needs `parent = ..`, the route enum itself",
        )),
    }
}

/// Field type of a tuple variant `Foo(Bar)`.
///
/// # Errors
///
/// Errors on any other variant shape (unit, struct, or multi-field).
pub fn single_field_ty(fields: &Fields) -> syn::Result<Type> {
    match fields {
        Fields::Unnamed(u) if u.unnamed.len() == 1 => Ok(u.unnamed[0].ty.clone()),
        _ => Err(syn::Error::new(
            fields.span(),
            "expected a single-field tuple variant `Foo(Bar)`",
        )),
    }
}

/// `(inner, true)` if `ty` is `Box<T>`, else `(ty, false)`. A recursive field uses `Box`; the projection must dereference.
#[must_use]
pub fn unbox(ty: &Type) -> (&Type, bool) {
    if let Type::Path(tp) = ty
        && let Some(seg) = tp.path.segments.last()
        && seg.ident == "Box"
        && let syn::PathArguments::AngleBracketed(args) = &seg.arguments
        && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
    {
        return (inner, true);
    }
    (ty, false)
}

/// True when the node carries `#[node(root)]`.
#[must_use]
pub fn is_root(attrs: &[syn::Attribute]) -> bool {
    for attr in attrs {
        if attr.path().is_ident("node") {
            let mut root = false;
            let _ = attr.parse_nested_meta(|m| {
                if m.path.is_ident("root") {
                    root = true;
                    Ok(())
                } else if m.path.is_ident("parent_path") {
                    let _: Path = m.value()?.parse()?;
                    Ok(())
                } else {
                    Err(m.error("expected `root` or `parent`"))
                }
            });
            if root {
                return true;
            }
        }
    }
    false
}

/// Parent path from `#[node(parent_path = P)]`. `None` for `#[node(root)]` or a missing `#[node(..)]`.
///
/// # Errors
///
/// Errors if `#[node(..)]` is present without `parent_path = ..` or `root`.
pub fn node_parent(attrs: &[syn::Attribute]) -> syn::Result<Option<Path>> {
    for attr in attrs {
        if attr.path().is_ident("node") {
            let mut parent = None;
            let mut root = false;
            attr.parse_nested_meta(|m| {
                if m.path.is_ident("parent_path") {
                    parent = Some(m.value()?.parse()?);
                    Ok(())
                } else if m.path.is_ident("root") {
                    root = true;
                    Ok(())
                } else {
                    Err(m.error("expected `parent = ..` or `root`"))
                }
            })?;
            if root {
                return Ok(None);
            }
            return Ok(Some(parent.ok_or_else(|| {
                syn::Error::new(attr.span(), "`#[node(..)]` needs `parent = ..` or `root`")
            })?));
        }
    }
    Ok(None)
}

pub enum Via<'a> {
    Field(&'a Member),
    Variant(&'a Ident),
}

/// Descent edge from a parent node to a child. Shared by `resolve` and `dispatch` so both build the same child path.
pub struct Edge<'a> {
    pub parent: &'a Ident,
    pub is_root: bool,
    pub route: Option<&'a Route>,
    pub boxed: bool,
    pub via: Via<'a>,
}

impl Edge<'_> {
    /// Child `Path` expression from the parent-path expression `path`.
    // `match` rather than `map_or_else`: the `quote!` arms are multi-line.
    #[expect(clippy::option_if_let_else)]
    #[must_use]
    pub fn child_path(&self, path: &TokenStream2) -> TokenStream2 {
        let deref = if self.boxed { quote!(*) } else { quote!() };
        match self.route {
            // Single-parent: `from_fn` already builds `Path<Child, ThisPath>`, so `.into()` is identity.
            None => {
                let (project, project_ref) = self.single_parent_projection(&deref);
                quote!(::laserbeam::PathMut::from_fn(#path, #project, #project_ref).into())
            }
            // Wrap this node's path in the route variant named after this node.
            Some(route) => {
                let parent = self.parent;
                let route = &route.parent;
                let variant = quote!(#route::#parent);
                let (project, project_ref) = self.multi_parent_projection(&variant, &deref);
                quote!(::laserbeam::PathMut::from_fn(
                    #variant(#path.into()),
                    #project,
                    #project_ref
                ))
            }
        }
    }

    /// Recover this node's path from child path `child`. A routed child's `into_parent` is the route enum, matched back to this node's variant.
    #[expect(clippy::option_if_let_else)]
    #[must_use]
    pub fn recover_parent(&self, child: &TokenStream2) -> TokenStream2 {
        match self.route {
            None => quote!(#child.into_parent()),
            Some(route) => {
                let parent = self.parent;
                let route = &route.parent;
                let variant = quote!(#route::#parent);
                quote!({
                    let #variant(pp) = #child.into_parent() else { ::core::unreachable!() };
                    pp
                })
            }
        }
    }

    /// Mutable and shared projection closures. A path stores both because the mutable one cannot run through a shared borrow.
    fn single_parent_projection(&self, deref: &TokenStream2) -> (TokenStream2, TokenStream2) {
        match &self.via {
            Via::Field(field) => {
                if self.is_root {
                    (
                        quote!(|o| &mut #deref o.#field),
                        quote!(|o| & #deref o.#field),
                    )
                } else {
                    (
                        quote!(|np| &mut #deref np.get_mut().#field),
                        quote!(|np| & #deref np.get().#field),
                    )
                }
            }
            Via::Variant(vi) => {
                let (access, access_ref) = if self.boxed {
                    (quote!(&mut **c), quote!(&**c))
                } else {
                    (quote!(c), quote!(c))
                };
                if self.is_root {
                    (
                        quote!(|o| {
                            let Self::#vi(c) = &mut **o else { ::core::unreachable!() };
                            #access
                        }),
                        quote!(|o| {
                            let Self::#vi(c) = &**o else { ::core::unreachable!() };
                            #access_ref
                        }),
                    )
                } else {
                    (
                        quote!(|np| {
                            let Self::#vi(c) = np.get_mut() else { ::core::unreachable!() };
                            #access
                        }),
                        quote!(|np| {
                            let Self::#vi(c) = np.get() else { ::core::unreachable!() };
                            #access_ref
                        }),
                    )
                }
            }
        }
    }

    /// Projection through the live route variant. Other variants of the route are `unreachable!`.
    fn multi_parent_projection(
        &self,
        variant: &TokenStream2,
        deref: &TokenStream2,
    ) -> (TokenStream2, TokenStream2) {
        match &self.via {
            Via::Field(field) => {
                let (node, node_ref) = if self.is_root {
                    (quote!(pp.#field), quote!(pp.#field))
                } else {
                    (quote!(pp.get_mut().#field), quote!(pp.get().#field))
                };
                (
                    quote!(|p| {
                        let #variant(pp) = p else { ::core::unreachable!() };
                        &mut #deref #node
                    }),
                    quote!(|p| {
                        let #variant(pp) = p else { ::core::unreachable!() };
                        & #deref #node_ref
                    }),
                )
            }
            Via::Variant(vi) => {
                let (inner, inner_ref) = if self.boxed {
                    (quote!(&mut **inner), quote!(&**inner))
                } else {
                    (quote!(inner), quote!(inner))
                };
                let (node, node_ref) = if self.is_root {
                    (quote!(&mut **pp), quote!(&**pp))
                } else {
                    (quote!(pp.get_mut()), quote!(pp.get()))
                };
                (
                    quote!(|p| {
                        let #variant(pp) = p else { ::core::unreachable!() };
                        let Self::#vi(inner) = #node else { ::core::unreachable!() };
                        #inner
                    }),
                    quote!(|p| {
                        let #variant(pp) = p else { ::core::unreachable!() };
                        let Self::#vi(inner) = #node_ref else { ::core::unreachable!() };
                        #inner_ref
                    }),
                )
            }
        }
    }
}
