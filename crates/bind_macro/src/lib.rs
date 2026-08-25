//! `#[derive(Bind)]`: implements `Dispatch` and, under `check`, `AccumulateTriggers`.

use derive_support::{
    Edge, Route, Via, find_children, is_root, node_parent, parent_route, single_field_ty, unbox,
};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{Data, DeriveInput, Expr, Fields, Ident, Path, Token, Type, parse_macro_input};

#[proc_macro_derive(
    Bind,
    attributes(
        binds,
        bind,
        post,
        pre_post,
        child,
        derived_children,
        derived_node,
        node
    )
)]
pub fn derive_bind(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand(input: &DeriveInput) -> syn::Result<TokenStream2> {
    if input.generics.where_clause.is_some() {
        return Err(syn::Error::new(
            input.generics.span(),
            "a bind node may not carry its own where clause",
        ));
    }
    let name = &input.ident;
    let marker = marker_of(input)?;
    let items = scheduled(&input.attrs)?;

    // Derived levels have no `HasPath`. They implement `DispatchIntoTreePath` on `DerivedLevel` instead.
    if let Some(parent) = derived_node_parent(&input.attrs)? {
        if !input.generics.params.is_empty() {
            return Err(syn::Error::new(
                input.generics.span(),
                "a derived level may not be generic",
            ));
        }
        return derived_node_impl(input, name, &parent, &marker, &items);
    }

    let place = place_impl(input, name)?;
    let accumulate = accumulate_impl(input, name, &marker, &items)?;
    let dispatch = dispatch_impl(input, name, &marker, &items)?;
    Ok(quote! {
        #place
        #accumulate
        #dispatch
    })
}

/// `HasPath` for a place node: `PathMut<Self, Parent>` or `&mut Self` at the root.
fn place_impl(input: &DeriveInput, name: &Ident) -> syn::Result<TokenStream2> {
    let path_ty = if is_root(&input.attrs) {
        quote!(&'a mut Self)
    } else {
        let parent = node_parent(&input.attrs)?.ok_or_else(|| {
            syn::Error::new(
                input.ident.span(),
                "a bind node needs `#[node(parent_path = ..)]` or `#[node(root)]`",
            )
        })?;
        quote!(::laserbeam::PathMut<Self, #parent<'a>>)
    };
    let (impl_g, ty_g, _) = input.generics.split_for_impl();
    Ok(quote! {
        #[automatically_derived]
        impl #impl_g ::laserbeam::HasPath for #name #ty_g {
            type Path<'a>
                = #path_ty
            where
                Self: 'a;
        }
    })
}

/// Parent path from `#[derived_node(parent_path = Alias)]`. The derive cannot see the parent, so the attribute names it.
fn derived_node_parent(attrs: &[syn::Attribute]) -> syn::Result<Option<Path>> {
    let mut found = None;
    for attr in attrs {
        if attr.path().is_ident("derived_node") {
            if found.is_some() {
                return Err(syn::Error::new(
                    attr.span(),
                    "expected one `#[derived_node(..)]`",
                ));
            }
            let mut parent = None;
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("parent_path") {
                    parent = Some(meta.value()?.parse::<Path>()?);
                    Ok(())
                } else {
                    Err(meta.error("expected `parent_path = Alias`"))
                }
            })?;
            found = Some(parent.ok_or_else(|| {
                syn::Error::new(attr.span(), "`#[derived_node]` needs `parent_path = Alias`")
            })?);
        }
    }
    Ok(found)
}

/// Fns from `#[derived_children(f, g)]`, listed order. Each is `fn(&Parent) -> Option<Data>`.
fn derived_children_fns(attrs: &[syn::Attribute]) -> syn::Result<Vec<Path>> {
    let mut found: Option<Vec<Path>> = None;
    for attr in attrs {
        if attr.path().is_ident("derived_children") {
            if found.is_some() {
                return Err(syn::Error::new(
                    attr.span(),
                    "expected one `#[derived_children(..)]`",
                ));
            }
            let fns: Vec<Path> = attr
                .parse_args_with(Punctuated::<Path, Token![,]>::parse_terminated)?
                .into_iter()
                .collect();
            if fns.is_empty() {
                return Err(syn::Error::new(
                    attr.span(),
                    "`#[derived_children(..)]` names at least one fn",
                ));
            }
            found = Some(fns);
        }
    }
    Ok(found.unwrap_or_default())
}

/// At most one derived child. The level's `data` dies with the dispatch, so a second sibling would have no parent to recover.
fn derived_level_child(attrs: &[syn::Attribute]) -> syn::Result<Option<Path>> {
    let mut fns = derived_children_fns(attrs)?;
    if fns.len() > 1 {
        return Err(syn::Error::new(
            fns[1].span(),
            "a derived level has at most one derived child",
        ));
    }
    Ok(fns.pop())
}

/// A place node's `#[child]` fields (declaration order) and `#[derived_children]` fns (listed order).
fn place_child_edges(input: &DeriveInput) -> syn::Result<(Vec<derive_support::Child>, Vec<Path>)> {
    let derived = derived_children_fns(&input.attrs)?;
    let fields = match &input.data {
        Data::Struct(s) => find_children(&s.fields)?,
        Data::Enum(_) if !derived.is_empty() => {
            return Err(syn::Error::new(
                input.span(),
                "an enum place node has no derived children; hang them under its variants' nodes",
            ));
        }
        _ => Vec::new(),
    };
    if fields.len() + derived.len() > 1 {
        let params: Vec<&Ident> = input.generics.type_params().map(|p| &p.ident).collect();
        for (_, ty, route) in &fields {
            let (child, _) = unbox(ty);
            if route.is_some() {
                return Err(syn::Error::new(
                    child.span(),
                    "a routed child may not share a node with other children",
                ));
            }
            if mentions_param(child, &params) {
                return Err(syn::Error::new(
                    child.span(),
                    "a generic child may not share a node with other children",
                ));
            }
        }
    }
    Ok((fields, derived))
}

/// One dispatch/accumulate arm per enum variant, each rebuilt with that variant's `Data` and the shared parent.
fn derived_enum_node_impl(
    input: &DeriveInput,
    name: &Ident,
    parent: &Path,
    marker: &Path,
    items: &[Scheduled],
    e: &syn::DataEnum,
) -> syn::Result<TokenStream2> {
    if !items.is_empty() {
        return Err(syn::Error::new(
            input.span(),
            "an enum of derived levels binds nothing itself; put the binds on its variants",
        ));
    }
    let mut dispatch_arms = Vec::new();
    let mut acc_arms = Vec::new();
    for v in &e.variants {
        let vi = &v.ident;
        single_field_ty(&v.fields)?;
        reject_child(&v.fields)?;
        dispatch_arms.push(quote! {
            #name::#vi(data) => ::bind::DispatchIntoTreePath::<#marker>::dispatch_into_tree_path(
                ::bind::DerivedLevel { parent, data },
                event,
                effs,
                claim,
            ),
        });
        acc_arms.push(quote! {
            #name::#vi(data) => ::bind::AccumulateDerivedTriggers::<#marker>::accumulate(
                ::bind::DerivedLevel { parent, data },
                out,
            ),
        });
    }
    Ok(quote! {
        #[automatically_derived]
        impl<'a> ::bind::DispatchIntoTreePath<#marker> for ::bind::DerivedLevel<#parent<'a>, #name> {
            fn dispatch_into_tree_path(
                self,
                event: &<#marker as ::bind::Bindings>::Event,
                effs: &mut <#marker as ::bind::Bindings>::Output,
                claim: &mut ::bind::Claim<'_>,
            ) -> ::laserbeam::Completed<
                <::bind::DerivedLevel<#parent<'a>, #name> as ::bind::HasTreePath>::TreePath,
            > {
                let ::bind::DerivedLevel { parent, data } = self;
                match data { #(#dispatch_arms)* }
            }
        }

        ::bind::check_only! {
        #[automatically_derived]
        #[expect(clippy::implicit_hasher)]
        impl<'a> ::bind::AccumulateDerivedTriggers<#marker> for ::bind::DerivedLevel<#parent<'a>, #name> {
            type Parent = #parent<'a>;

            fn accumulate(
                self,
                out: &mut ::std::collections::HashSet<
                    <#marker as ::bind::Bindings>::Trigger,
                >,
            ) -> ::core::result::Result<#parent<'a>, ::bind::BindError> {
                let ::bind::DerivedLevel { parent, data } = self;
                match data { #(#acc_arms)* }
            }
        }
        }
    })
}

/// `DispatchIntoTreePath` for a derived level. Node and place types go through `HasTreePath` because a parent that is itself derived has no place type the derive can name.
fn derived_node_impl(
    input: &DeriveInput,
    name: &Ident,
    parent: &Path,
    marker: &Path,
    items: &[Scheduled],
) -> syn::Result<TokenStream2> {
    if let Data::Enum(e) = &input.data {
        return derived_enum_node_impl(input, name, parent, marker, items, e);
    }
    if let Data::Struct(s) = &input.data {
        reject_child(&s.fields)?;
    }

    let node = quote!(node);
    let state = derived_state(input, marker, &node)?;
    let acc_descend = derived_accumulate_descent(input, marker)?;
    let opts = items.iter().enumerate().map(|(i, it)| opt(i, it, &node));
    let blocks = items
        .iter()
        .enumerate()
        .map(|(i, it)| scheduled_block(i, it));
    let binding = state_binding(items, false);
    let triggers = claimed_triggers(items);
    Ok(quote! {
        #[automatically_derived]
        impl<'a> ::bind::DispatchIntoTreePath<#marker> for ::bind::DerivedLevel<#parent<'a>, #name> {
            fn dispatch_into_tree_path(
                self,
                event: &<#marker as ::bind::Bindings>::Event,
                effs: &mut <#marker as ::bind::Bindings>::Output,
                claim: &mut ::bind::Claim<'_>,
            ) -> ::laserbeam::Completed<
                <::bind::DerivedLevel<#parent<'a>, #name> as ::bind::HasTreePath>::TreePath,
            > {
                let node = self;
                #(#opts)*
                let #binding = #state;
                #(#blocks)*
                ::laserbeam::MaybeInvalidated::complete(state)
            }
        }

        ::bind::check_only! {
        #[automatically_derived]
        #[expect(clippy::useless_conversion, clippy::implicit_hasher)]
        impl<'a> ::bind::AccumulateDerivedTriggers<#marker> for ::bind::DerivedLevel<#parent<'a>, #name> {
            type Parent = #parent<'a>;

            fn accumulate(
                self,
                out: &mut ::std::collections::HashSet<<#marker as ::bind::Bindings>::Trigger>,
            ) -> ::core::result::Result<#parent<'a>, ::bind::BindError> {
                let node = self;
                #(
                    ::bind::insert_or_error(out, ::core::convert::Into::into(#triggers))?;
                )*
                #acc_descend
                ::core::result::Result::Ok(node.parent)
            }
        }
        }
    })
}

/// Dispatch descent for a `#[derived_children]` fn. The fn takes `&Parent` so existence is decided before the parent is moved.
fn derived_child_state(f: &Path, marker: &Path, place: &TokenStream2) -> TokenStream2 {
    quote! {
        match #f(&#place) {
            ::core::option::Option::Some(data) => ::laserbeam::Completed::to_maybe_invalidated(
                ::bind::DispatchIntoTreePath::<#marker>::dispatch_into_tree_path(
                    ::bind::DerivedLevel { parent: #place, data },
                    event,
                    effs,
                    claim,
                ),
            ),
            ::core::option::Option::None => ::laserbeam::MaybeInvalidated::NotInvalidated(
                ::bind::HasTreePath::into_tree_path(#place),
            ),
        }
    }
}

/// Derived level's post-descent state: the child edge if any, otherwise the node flattened to its place.
fn derived_state(
    input: &DeriveInput,
    marker: &Path,
    node: &TokenStream2,
) -> syn::Result<TokenStream2> {
    Ok(derived_level_child(&input.attrs)?.map_or_else(
        || quote!(::laserbeam::MaybeInvalidated::NotInvalidated(::bind::HasTreePath::into_tree_path(#node))),
        |f| derived_child_state(&f, marker, node),
    ))
}

/// A derived level cannot hang a place child: its `data` dies with the dispatch, so a place below it would have to fold through a `DerivedLevel`, which is not a path.
fn reject_child(fields: &Fields) -> syn::Result<()> {
    for f in fields {
        if let Some(attr) = f.attrs.iter().find(|a| a.path().is_ident("child")) {
            return Err(syn::Error::new(
                attr.span(),
                "a derived level cannot have a `#[child]` field: its `data` dies with the \
                 dispatch. Persist the state in the tree at a real place the derived level reads, \
                 or hang a fresh level with `#[derived_children]`.",
            ));
        }
    }
    Ok(())
}

/// Accumulate descent for a `#[derived_children]` fn.
fn derived_child_accumulate(f: &Path, marker: &Path, place: &TokenStream2) -> TokenStream2 {
    quote! {
        let #place = match #f(&#place) {
            ::core::option::Option::Some(data) => {
                ::bind::AccumulateDerivedTriggers::<#marker>::accumulate(
                    ::bind::DerivedLevel { parent: #place, data },
                    out,
                )?
            }
            ::core::option::Option::None => #place,
        };
    }
}

fn derived_accumulate_descent(input: &DeriveInput, marker: &Path) -> syn::Result<TokenStream2> {
    Ok(derived_level_child(&input.attrs)?.map_or_else(
        || quote!(),
        |f| derived_child_accumulate(&f, marker, &quote!(node)),
    ))
}

/// `AccumulateTriggers` impl. A node with several children returns `Err(BindError::MultiChildNode)`.
fn accumulate_impl(
    input: &DeriveInput,
    name: &Ident,
    marker: &Path,
    items: &[Scheduled],
) -> syn::Result<TokenStream2> {
    let root = is_root(&input.attrs);
    let (fields, derived) = place_child_edges(input)?;
    if fields.len() + derived.len() > 1 {
        let (impl_g, ty_g, _) = input.generics.split_for_impl();
        return Ok(quote! {
            ::bind::check_only! {
            #[automatically_derived]
            #[expect(clippy::implicit_hasher)]
            impl #impl_g ::bind::AccumulateTriggers<#marker> for #name #ty_g {
                fn accumulate<'a>(
                    _path: <Self as ::laserbeam::HasPath>::Path<'a>,
                    _out: &mut ::std::collections::HashSet<<#marker as ::bind::Bindings>::Trigger>,
                ) -> ::core::result::Result<
                    <Self as ::laserbeam::HasPath>::Path<'a>,
                    ::bind::BindError,
                >
                where
                    Self: 'a,
                {
                    ::core::result::Result::Err(::bind::BindError::MultiChildNode)
                }
            }
            }
        });
    }
    let (recurse, children, needs_mut) =
        accumulate_body(input, name, marker, root, &fields, &derived)?;
    let where_clause = child_where_clause(
        input,
        &children,
        &quote!(::bind::AccumulateTriggers<#marker>),
    );
    let binding = if needs_mut {
        quote!(mut path)
    } else {
        quote!(path)
    };
    let (impl_g, ty_g, _) = input.generics.split_for_impl();
    let triggers = claimed_triggers(items);
    Ok(quote! {
        ::bind::check_only! {
        #[automatically_derived]
        #[expect(clippy::useless_conversion, clippy::implicit_hasher)]
        impl #impl_g ::bind::AccumulateTriggers<#marker> for #name #ty_g #where_clause {
            fn accumulate<'a>(
                #binding: <Self as ::laserbeam::HasPath>::Path<'a>,
                out: &mut ::std::collections::HashSet<<#marker as ::bind::Bindings>::Trigger>,
            ) -> ::core::result::Result<
                <Self as ::laserbeam::HasPath>::Path<'a>,
                ::bind::BindError,
            >
            where
                Self: 'a,
            {
                #(
                    ::bind::insert_or_error(out, ::core::convert::Into::into(#triggers))?;
                )*
                #recurse
                ::core::result::Result::Ok(path)
            }
        }
        }
    })
}

/// Recursion, child types to bound, and whether `path` needs `mut`. Only for a node with at most one child edge.
fn accumulate_body(
    input: &DeriveInput,
    name: &Ident,
    marker: &Path,
    root: bool,
    fields: &[derive_support::Child],
    derived: &[Path],
) -> syn::Result<(TokenStream2, Vec<Type>, bool)> {
    if let Some(f) = derived.first() {
        return Ok((
            derived_child_accumulate(f, marker, &quote!(path)),
            Vec::new(),
            false,
        ));
    }
    match &input.data {
        Data::Struct(_) => match fields.first() {
            None => Ok((quote!(), Vec::new(), false)),
            Some((field, child_ty, route)) => {
                let (child, boxed) = unbox(child_ty);
                reject_routed_generic(input, child, route.as_ref())?;
                let edge = Edge {
                    parent: name,
                    is_root: root,
                    route: route.as_ref(),
                    boxed,
                    via: Via::Field(field),
                };
                let child_path = edge.child_path(&quote!(path));
                let recover = edge.recover_parent(&quote!(child));
                let recurse = quote! {
                    let child =
                        <#child as ::bind::AccumulateTriggers<#marker>>::accumulate(#child_path, out)?;
                    path = #recover;
                };
                Ok((recurse, vec![child.clone()], true))
            }
        },
        Data::Enum(e) => {
            let mut arms = Vec::new();
            let mut children = Vec::new();
            for v in &e.variants {
                let vi = &v.ident;
                let ty = single_field_ty(&v.fields)?;
                let route = parent_route(&v.attrs)?;
                let (child, boxed) = unbox(&ty);
                children.push(child.clone());
                let edge = Edge {
                    parent: name,
                    is_root: root,
                    route: route.as_ref(),
                    boxed,
                    via: Via::Variant(vi),
                };
                let child_path = edge.child_path(&quote!(path));
                let recover = edge.recover_parent(&quote!(child));
                arms.push(quote! {
                    Self::#vi(_) => {
                        let child = <#child as ::bind::AccumulateTriggers<#marker>>::accumulate(
                            #child_path,
                            out,
                        )?;
                        path = #recover;
                    }
                });
            }
            let scrutinee = if root {
                quote!(path)
            } else {
                quote!(path.get_mut())
            };
            Ok((quote!(match #scrutinee { #(#arms)* }), children, true))
        }
        Data::Union(_) => Err(syn::Error::new(
            input.span(),
            "bind does not support unions",
        )),
    }
}

fn dispatch_impl(
    input: &DeriveInput,
    name: &Ident,
    marker: &Path,
    items: &[Scheduled],
) -> syn::Result<TokenStream2> {
    let root = is_root(&input.attrs);
    let path = quote!(path);
    let (init, child_blocks, children) = dispatch_state(input, name, marker, root, &path)?;
    let where_clause = child_where_clause(input, &children, &quote!(::bind::Dispatch<#marker>));
    let opts = items.iter().enumerate().map(|(i, it)| opt(i, it, &path));
    let blocks = items
        .iter()
        .enumerate()
        .map(|(i, it)| scheduled_block(i, it));
    let binding = state_binding(items, !child_blocks.is_empty());
    let (impl_g, ty_g, _) = input.generics.split_for_impl();
    Ok(quote! {
        #[automatically_derived]
        impl #impl_g ::bind::Dispatch<#marker> for #name #ty_g #where_clause {
            fn dispatch<'a, 'c>(
                path: <Self as ::laserbeam::HasPath>::Path<'a>,
                event: &<#marker as ::bind::Bindings>::Event,
                effs: &mut <#marker as ::bind::Bindings>::Output,
                claim: &mut ::bind::Claim<'c>,
            ) -> ::laserbeam::Completed<<Self as ::laserbeam::HasPath>::Path<'a>>
            where
                Self: 'a,
                <Self as ::laserbeam::HasPath>::Path<'a>: ::laserbeam::HasStop,
            {
                #(#opts)*
                let #binding = #init;
                #(#child_blocks)*
                #(#blocks)*
                ::laserbeam::MaybeInvalidated::complete(state)
            }
        }
    })
}

/// Child trait bounds. A child type that names a type parameter also gets a `Path` equality so the generated `PathMut` is the child's `HasPath::Path`.
fn child_where_clause(
    input: &DeriveInput,
    children: &[Type],
    bound: &TokenStream2,
) -> TokenStream2 {
    if children.is_empty() {
        return quote!();
    }
    let params: Vec<&Ident> = input.generics.type_params().map(|p| &p.ident).collect();
    let preds = children.iter().map(|child| {
        if mentions_param(child, &params) {
            quote! {
                #child: 'static + #bound,
                for<'q> #child: ::laserbeam::HasPath<
                    Path<'q> = ::laserbeam::PathMut<
                        #child,
                        <Self as ::laserbeam::HasPath>::Path<'q>,
                    >,
                >,
            }
        } else {
            quote!(#child: #bound,)
        }
    });
    quote!(where #(#preds)*)
}

/// Routed children cannot be generic: recover names a parent variant.
fn reject_routed_generic(
    input: &DeriveInput,
    child: &Type,
    route: Option<&Route>,
) -> syn::Result<()> {
    let params: Vec<&Ident> = input.generics.type_params().map(|p| &p.ident).collect();
    if route.is_some() && mentions_param(child, &params) {
        return Err(syn::Error::new(
            child.span(),
            "a routed (multi-parent) child may not be generic",
        ));
    }
    Ok(())
}

fn mentions_param(ty: &Type, params: &[&Ident]) -> bool {
    fn walk(ts: TokenStream2, params: &[&Ident], hit: &mut bool) {
        for tt in ts {
            match tt {
                ::proc_macro2::TokenTree::Ident(i) => {
                    if params.iter().any(|p| **p == i) {
                        *hit = true;
                    }
                }
                ::proc_macro2::TokenTree::Group(g) => walk(g.stream(), params, hit),
                _ => {}
            }
        }
    }
    let mut hit = false;
    walk(quote!(#ty), params, &mut hit);
    hit
}

/// `mut state` only when a child block or scheduled item rebinds it.
fn state_binding(items: &[Scheduled], has_child_blocks: bool) -> TokenStream2 {
    if items.is_empty() && !has_child_blocks {
        quote!(state)
    } else {
        quote!(mut state)
    }
}

/// Init, then one `descend` block per child: `#[child]` fields in declaration order, then `#[derived_children]` fns in listed order.
fn dispatch_state(
    input: &DeriveInput,
    name: &Ident,
    marker: &Path,
    root: bool,
    place: &TokenStream2,
) -> syn::Result<(TokenStream2, Vec<TokenStream2>, Vec<Type>)> {
    let (fields, derived) = place_child_edges(input)?;
    let mut blocks = Vec::new();
    let mut children = Vec::new();
    let init = match &input.data {
        Data::Struct(_) => {
            for (field, child_ty, route) in &fields {
                let (child, boxed) = unbox(child_ty);
                reject_routed_generic(input, child, route.as_ref())?;
                let edge = Edge {
                    parent: name,
                    is_root: root,
                    route: route.as_ref(),
                    boxed,
                    via: Via::Field(field),
                };
                let fold = child_state(&edge, child, marker, place);
                blocks.push(quote! {
                    state = ::laserbeam::MaybeInvalidated::descend(state, |#place| #fold);
                });
                children.push(child.clone());
            }
            quote!(::laserbeam::MaybeInvalidated::NotInvalidated(#place))
        }
        Data::Enum(e) => {
            let mut arms = Vec::new();
            for v in &e.variants {
                let vi = &v.ident;
                let ty = single_field_ty(&v.fields)?;
                let route = parent_route(&v.attrs)?;
                let (child, boxed) = unbox(&ty);
                children.push(child.clone());
                let edge = Edge {
                    parent: name,
                    is_root: root,
                    route: route.as_ref(),
                    boxed,
                    via: Via::Variant(vi),
                };
                let state = child_state(&edge, child, marker, place);
                arms.push(quote!(Self::#vi(_) => { #state }));
            }
            // Root matches `&mut Self`; a non-root matches `path.get()`. Shared, because the arm only needs the discriminant, then consumes the path.
            let scrutinee = if root {
                quote!(#place)
            } else {
                quote!(#place.get())
            };
            quote!(match #scrutinee { #(#arms)* })
        }
        Data::Union(_) => {
            return Err(syn::Error::new(
                input.span(),
                "bind does not support unions",
            ));
        }
    };
    // Derived children are fns, not fields. The derive has only `f`'s name, not the child's type.
    for f in &derived {
        let fold = derived_child_state(f, marker, place);
        blocks.push(quote! {
            state = ::laserbeam::MaybeInvalidated::descend(state, |#place| #fold);
        });
    }
    Ok((init, blocks, children))
}

/// Dispatch a place child and fold its leave. A routed child's `Up` is the consumer's enum, so the fold matches the live variant; `unreachable!` is the other routes.
fn child_state(edge: &Edge<'_>, child: &Type, marker: &Path, place: &TokenStream2) -> TokenStream2 {
    let child_path = edge.child_path(place);
    let leave = quote! {
        ::laserbeam::Completed::into_inner(
            <#child as ::bind::Dispatch<#marker>>::dispatch(#child_path, event, effs, claim),
        )
    };
    let Some(Route { parent: route, up }) = edge.route else {
        return quote!(#leave.to_maybe_invalidated());
    };
    let parent = edge.parent;
    quote! {
        match #leave {
            ::laserbeam::Stop::Here(child) => {
                let #route::#parent(recovered) = child.into_parent() else {
                    ::core::unreachable!()
                };
                ::laserbeam::MaybeInvalidated::NotInvalidated(recovered)
            }
            ::laserbeam::Stop::Up(above) => {
                let #up::#parent(completed) = above else { ::core::unreachable!() };
                ::laserbeam::MaybeInvalidated::Invalidated(completed)
            }
        }
    }
}

fn marker_of(input: &DeriveInput) -> syn::Result<Path> {
    let mut found = None;
    for attr in &input.attrs {
        if attr.path().is_ident("binds") {
            if found.is_some() {
                return Err(syn::Error::new(attr.span(), "expected one `#[binds(..)]`"));
            }
            found = Some(attr.parse_args::<Path>()?);
        }
    }
    found.ok_or_else(|| syn::Error::new(input.span(), "missing `#[binds(Marker)]`"))
}

/// Trigger expression. Closures go through `call_with` so the parameter infers. The distinction is syntactic because a trait cannot separate values from closures. Shared borrow so the trigger cannot write the node.
fn trigger_expr(trigger: &Expr, state: &TokenStream2) -> TokenStream2 {
    if matches!(trigger, Expr::Closure(_)) {
        quote!(::bind::call_with(&#state, #trigger))
    } else {
        quote!(#trigger)
    }
}

/// Triggers the check collects: `#[bind]` only, and not closures. A closure is read from state at dispatch, so it is not a static claim.
fn claimed_triggers(items: &[Scheduled]) -> impl Iterator<Item = &Expr> {
    items
        .iter()
        .filter(|it| it.claims && !matches!(it.trigger, Expr::Closure(_)))
        .map(|it| &it.trigger)
}

fn opt_ident(i: usize) -> Ident {
    format_ident!("opt_{i}")
}

/// Snap this item's trigger and pre before descent, while the child is still there. The pre is always called, including the synthesized `|_, _| ()`, so every item has the same shape.
fn opt(i: usize, item: &Scheduled, state: &TokenStream2) -> TokenStream2 {
    let ident = opt_ident(i);
    let trigger = trigger_expr(&item.trigger, state);
    let pre = &item.pre;
    quote! {
        let #ident = match ::core::convert::TryFrom::try_from(event) {
            ::core::result::Result::Ok(ev) => {
                let trigger = #trigger;
                if ::bind::EventTrigger::is_matching(&trigger, ev) {
                    ::core::option::Option::Some((ev, (#pre)(ev, &#state)))
                } else {
                    ::core::option::Option::None
                }
            }
            ::core::result::Result::Err(_) => ::core::option::Option::None,
        };
    }
}

/// Run the item if its opt matched. Does not branch on claim or invalidation; the handler does.
fn scheduled_block(i: usize, item: &Scheduled) -> TokenStream2 {
    let ident = opt_ident(i);
    let rhs = item.rhs();
    quote! {
        if let ::core::option::Option::Some((ev, snap)) = #ident {
            let (e, completed) = (#rhs)(
                ev,
                snap,
                ::bind::AscendState::new(state, ::bind::Claim::reborrow(claim)),
            );
            ::core::iter::Extend::extend(effs, e);
            state = ::laserbeam::Completed::to_maybe_invalidated(completed);
        }
    }
}

/// Pre for `#[bind]` and `#[post]`: snap is `()`.
fn unit_pre() -> Expr {
    syn::parse_quote!(|_, _| ())
}

/// One scheduled item. The three attribute kinds collapse to this; the emit is the same after parse.
struct Scheduled {
    trigger: Expr,
    pre: Expr,
    handler: Expr,
    /// True for `#[bind]`. Only a bind takes the claim.
    claims: bool,
}

impl Scheduled {
    /// Bind wraps in `exclusive`; a post is the handler as written. The macro does not look inside the rhs.
    fn rhs(&self) -> TokenStream2 {
        let handler = &self.handler;
        if self.claims {
            quote!(::bind::exclusive(#handler))
        } else {
            quote!(#handler)
        }
    }
}

struct Pair {
    trigger: Expr,
    handler: Expr,
}

impl syn::parse::Parse for Pair {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let trigger = input.parse()?;
        input.parse::<Token![=>]>()?;
        let handler = input.parse()?;
        Ok(Self { trigger, handler })
    }
}

struct PrePost {
    trigger: Expr,
    pre: Expr,
    post: Expr,
}

impl syn::parse::Parse for PrePost {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let trigger = input.parse()?;
        input.parse::<Token![=>]>()?;
        let content;
        syn::parenthesized!(content in input);
        let pre = content.parse()?;
        content.parse::<Token![,]>()?;
        let post = content.parse()?;
        Ok(Self { trigger, pre, post })
    }
}

/// Scheduled items in source order. Each item sees the state the previous one left.
fn scheduled(attrs: &[syn::Attribute]) -> syn::Result<Vec<Scheduled>> {
    let mut out = Vec::new();
    for attr in attrs {
        let claims = if attr.path().is_ident("bind") {
            true
        } else if attr.path().is_ident("post") {
            false
        } else {
            if attr.path().is_ident("pre_post") {
                let parsed =
                    attr.parse_args_with(Punctuated::<PrePost, Token![,]>::parse_terminated)?;
                out.extend(parsed.into_iter().map(|p| Scheduled {
                    trigger: p.trigger,
                    pre: p.pre,
                    handler: p.post,
                    claims: false,
                }));
            }
            continue;
        };
        let parsed = attr.parse_args_with(Punctuated::<Pair, Token![,]>::parse_terminated)?;
        out.extend(parsed.into_iter().map(|p| Scheduled {
            trigger: p.trigger,
            pre: unit_pre(),
            handler: p.handler,
            claims,
        }));
    }
    Ok(out)
}
