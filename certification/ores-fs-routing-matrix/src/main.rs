use ores_api_docs::{validate_and_sort_fs_routes, FsRoute};

fn main() {
    certifies_framework_neutral_paths();
    certifies_optional_catch_all_expansion();
    certifies_api_handler_paths();
    certifies_stable_precedence();
    rejects_ambiguous_route_shapes();
    rejects_invalid_sources_and_segments();
    rejects_duplicate_params_and_nonfinal_catchalls();
    println!("fiducia-cloud-test ores filesystem routing matrix certification passed");
}

fn certifies_framework_neutral_paths() {
    let root = FsRoute::page("src/pages/page.rs").expect("root page");
    assert_eq!(root.canonical_path(), "/");
    assert_eq!(root.axum_paths(), vec!["/"]);
    assert_eq!(root.dioxus_paths(), vec!["/"]);

    let route = FsRoute::page("src/pages/orgs/[org_id]/packages/[...slug]/page.rs")
        .expect("dynamic catch-all page");
    assert_eq!(route.canonical_path(), "/orgs/{org_id}/packages/{*slug}");
    assert_eq!(route.axum_paths(), vec!["/orgs/{org_id}/packages/{*slug}"]);
    assert_eq!(route.dioxus_paths(), vec!["/orgs/:org_id/packages/:..slug"]);
}

fn certifies_optional_catch_all_expansion() {
    let route = FsRoute::page("src/pages/docs/[[...slug]]/page.rs")
        .expect("optional catch-all page");
    assert_eq!(route.canonical_path(), "/docs/{*slug?}");
    assert_eq!(route.axum_paths(), vec!["/docs", "/docs/{*slug}"]);
    assert_eq!(route.dioxus_paths(), vec!["/docs", "/docs/:..slug"]);
}

fn certifies_api_handler_paths() {
    let route = FsRoute::api_handler("src/routes/v1/evidence/[evidence_id]/route.rs")
        .expect("api handler route");
    assert_eq!(route.canonical_path(), "/v1/evidence/{evidence_id}");
    assert_eq!(route.axum_paths(), vec!["/v1/evidence/{evidence_id}"]);
}

fn certifies_stable_precedence() {
    let authored = [
        "src/pages/users/[[...optional]]/page.rs",
        "src/pages/users/[...rest]/page.rs",
        "src/pages/users/[id]/page.rs",
        "src/pages/users/new/page.rs",
        "src/pages/page.rs",
    ];
    let forward = validate_and_sort_fs_routes(
        authored
            .iter()
            .map(|source| FsRoute::page(*source).expect("valid route")),
    )
    .expect("forward ordering");
    let reverse = validate_and_sort_fs_routes(
        authored
            .iter()
            .rev()
            .map(|source| FsRoute::page(*source).expect("valid route")),
    )
    .expect("reverse ordering");
    let forward_paths: Vec<_> = forward.iter().map(FsRoute::canonical_path).collect();
    let reverse_paths: Vec<_> = reverse.iter().map(FsRoute::canonical_path).collect();
    assert_eq!(forward_paths, reverse_paths, "input order must not affect route order");
    assert_eq!(
        forward_paths,
        vec![
            "/",
            "/users/new",
            "/users/{id}",
            "/users/{*rest}",
            "/users/{*optional?}",
        ]
    );
}

fn rejects_ambiguous_route_shapes() {
    let error = validate_and_sort_fs_routes([
        FsRoute::page("src/pages/users/[id]/page.rs").unwrap(),
        FsRoute::page("src/pages/users/[slug]/page.rs").unwrap(),
    ])
    .expect_err("dynamic siblings with different parameter names must conflict")
    .to_string();
    assert!(error.contains("both match"), "{error}");

    let error = validate_and_sort_fs_routes([
        FsRoute::page("src/pages/files/[...path]/page.rs").unwrap(),
        FsRoute::page("src/pages/files/[...rest]/page.rs").unwrap(),
    ])
    .expect_err("catch-all siblings must conflict")
    .to_string();
    assert!(error.contains("both match"), "{error}");
}

fn rejects_invalid_sources_and_segments() {
    for source in [
        "src\\pages\\users\\page.rs",
        "src/pages/../secrets/page.rs",
        "pages/users/page.rs",
    ] {
        let error = FsRoute::page(source)
            .expect_err("unsafe route source must fail")
            .to_string();
        assert!(error.contains("cannot escape") || error.contains("route root"), "{error}");
    }

    let error = FsRoute::page("src/pages/users/not-page.txt")
        .expect_err("wrong special-file leaf must fail")
        .to_string();
    assert!(error.contains("must end in `page.rs`"), "{error}");

    for source in [
        "src/pages/users/{id}/page.rs",
        "src/pages/users/:id/page.rs",
        "src/pages/users/*id/page.rs",
        "src/pages/users/[]/page.rs",
        "src/pages/users/[bad name]/page.rs",
    ] {
        let error = FsRoute::page(source)
            .expect_err("invalid segment must fail")
            .to_string();
        assert!(error.contains("invalid route segment"), "{source}: {error}");
    }
}

fn rejects_duplicate_params_and_nonfinal_catchalls() {
    let error = FsRoute::page("src/pages/[id]/children/[id]/page.rs")
        .expect_err("duplicate parameter must fail")
        .to_string();
    assert!(error.contains("duplicate route parameter"), "{error}");

    let error = FsRoute::page("src/pages/files/[...rest]/edit/page.rs")
        .expect_err("catch-all must be final")
        .to_string();
    assert!(error.contains("must be the final route segment"), "{error}");

    let error = FsRoute::page("src/pages/files/[[...rest]]/edit/page.rs")
        .expect_err("optional catch-all must be final")
        .to_string();
    assert!(error.contains("must be the final route segment"), "{error}");
}
