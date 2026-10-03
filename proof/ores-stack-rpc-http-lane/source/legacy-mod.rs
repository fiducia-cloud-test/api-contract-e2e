include!("../main.rs");

#[derive(Debug, Default, Deserialize)]
struct DevLambdaResolvedValues {
    #[serde(rename = "ORES_STACK_DEV_WITH_LAMBDAS", default)]
    with_lambdas: bool,
}

fn render_help_if_requested(argv: &[String]) -> Result<bool, CliError> {
    let parser = BundledFlags2Env::new();
    let requested = parser
        .is_help_requested(argv)
        .map_err(|error| CliError::Usage(format!("help-token detection failed: {error}")))?;
    if !requested {
        return Ok(false);
    }

    // Help and parsing deliberately consume the same embedded .cli-flags.toml
    // authority. No second option/command schema lives in the Rust CLI.
    let contract = EmbeddedFlagContract::materialize()?;
    let contract_path = contract.path_str()?;
    let help = parser
        .help_table_for_argv("ores-stack", argv, 0, Some(contract_path))
        .map_err(|error| CliError::Usage(format!("help rendering failed: {error}")))?;
    print!("{help}");
    Ok(true)
}

fn resolve_dev_with_lambdas(argv: &[String]) -> Result<bool, CliError> {
    // Read the flag through the same embedded flags-2-env authority used by the
    // legacy parser. This helper adds no independent CLI schema.
    let contract = EmbeddedFlagContract::materialize()?;
    let contract_path = contract.path_str()?;
    let parser = BundledFlags2Env::new();
    let structured = parser
        .parse_structured(argv, Some(contract_path))
        .map_err(|_| CliError::Usage("flag parsing failed".to_owned()))?;
    let mut raw: HashMap<String, String> = structured.dotenv.clone();
    raw.extend(structured.dotenv_overrides.clone());
    raw.extend(std::env::vars());
    raw.extend(structured.provided_flags.clone());
    let values: DevLambdaResolvedValues = parser
        .coerce(&raw, Some(contract_path))
        .map_err(|_| CliError::Usage("typed dev Lambda flag coercion failed".to_owned()))?;
    Ok(values.with_lambdas)
}

pub(super) fn run_legacy(
    argv: &[String],
    context: super::SharedCliContext,
) -> std::process::ExitCode {
    let result = render_help_if_requested(argv).and_then(|help_rendered| {
        if help_rendered {
            return Ok(());
        }

        parse_invocation(argv).and_then(|mut invocation| {
            invocation.json = if context.output_was_explicit {
                context.runtime.json()
            } else {
                invocation.json || context.runtime.json()
            };

            let root = absolute_root(&invocation.root);
            match &invocation.command {
                CliCommand::Build { .. } | CliCommand::Check => {
                    let report = ores_stack_core::check_api_rpc_sources(&root).map_err(|error| {
                        CliError::Usage(format!(
                            "RPC handlers.rs/route.rs contract check failed before build: {error}"
                        ))
                    })?;
                    if report.checked_folders > 0 {
                        ores_stack_core::sync_api_rpc_sources(&root, true).map_err(|error| {
                            CliError::Usage(format!(
                                "generated route-local rpc.rs drift failed before build: {error}"
                            ))
                        })?;
                        ores_stack_core::sync_api_rpc_route_map(&root, true).map_err(|error| {
                            CliError::Usage(format!(
                                "generated RPC operation-index/HTTP-projection drift failed before build: {error}"
                            ))
                        })?;
                    }
                    let check_imports = matches!(&invocation.command, CliCommand::Check);
                    ores_stack_core::sync_rpc_client_import_aliases(&root, check_imports)
                        .map_err(|error| {
                            CliError::Usage(format!(
                                "parallel RPC client import-lane generation failed before build: {error}"
                            ))
                        })?;
                }
                CliCommand::Dev => match ores_stack_core::inspect_api_rpc_sources(&root) {
                    Ok(report) => {
                        let mut warned = false;
                        if !report.is_clean() {
                            warned = true;
                            eprintln!(
                                "ores-stack: warning: RPC source drift detected; dev will continue so unaffected routes can run"
                            );
                            for issue in &report.issues {
                                eprintln!("ores-stack: warning: {}: {}", issue.path, issue.message);
                            }
                        } else if report.checked_folders > 0 {
                            if let Err(error) = ores_stack_core::sync_api_rpc_sources(&root, true) {
                                warned = true;
                                eprintln!(
                                    "ores-stack: warning: generated route-local rpc.rs drift: {error}"
                                );
                            }
                            if let Err(error) = ores_stack_core::sync_api_rpc_route_map(&root, true) {
                                warned = true;
                                eprintln!(
                                    "ores-stack: warning: generated RPC operation-index/HTTP-projection drift: {error}"
                                );
                            }
                        }
                        if warned {
                            eprintln!(
                                "ores-stack: warning: run `ores-stack sync` to regenerate RPC glue, then `ores-stack sync --check`"
                            );
                        }
                    }
                    Err(error) => {
                        eprintln!(
                            "ores-stack: warning: RPC source preflight could not complete in dev: {error}"
                        );
                    }
                },
                _ => {}
            }

            match &invocation.command {
                CliCommand::Build { .. } => {
                    super::web_generation_watch::generate_web_sources(&root, false).map_err(
                        |error| {
                            CliError::Usage(format!(
                                "web page lambda.rs generation failed before build: {error}"
                            ))
                        },
                    )?;
                }
                CliCommand::Check => {
                    super::web_generation_watch::generate_web_sources(&root, true).map_err(
                        |error| {
                            CliError::Usage(format!(
                                "web page lambda.rs generated-source check failed: {error}"
                            ))
                        },
                    )?;
                }
                CliCommand::Dev => {
                    let with_lambdas = resolve_dev_with_lambdas(argv)?;
                    if with_lambdas {
                        match super::web_generation_watch::generate_web_sources(&root, false) {
                            Ok(_) => {}
                            Err(error) => eprintln!(
                                "ores-stack: warning: initial web page lambda generation failed; last generated files remain in place: {error}"
                            ),
                        }
                        if let Err(error) =
                            super::web_generation_watch::start_web_generation_watch(root.clone())
                        {
                            eprintln!("ores-stack: warning: {error}");
                        }
                    } else {
                        eprintln!(
                            "ores-stack: web Lambda generation disabled in dev; pass --with-lambdas to generate/watch sibling lambda.rs"
                        );
                    }
                }
                _ => {}
            }

            run(invocation)
        })
    });

    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            let diagnostic = ores_clis_core::paint(
                context.runtime.color_stderr(),
                ores_clis_core::ColorRole::Error,
                format_args!("ores-stack: {error}"),
            );
            eprintln!("{diagnostic}");
            std::process::ExitCode::FAILURE
        }
    }
}
