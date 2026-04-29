mod atom;
mod coordinator;
mod molecule;

use anyhow::Result;

fn main() -> Result<()> {
    coordinator::command_dispatch::run()
}

#[cfg(test)]
mod tests {
    use crate::atom::cli_contract::{Cli, Commands, DataCommands};
    use crate::atom::config_store::{load_cli_config, write_cli_config, CliConfig};
    use crate::atom::json_contract::StataPathSource;
    use crate::atom::path_ops::{
        absolutize_cli_path, default_config_path, discover_repo_root_from, normalize_repo_root,
    };
    use crate::atom::process_runner::inspect_python_version;
    use crate::molecule::backend_client::{
        base_backend_args, data_backend_invocation, project_python_for_tests, session_args,
    };
    use crate::molecule::repo_resolution::{resolve_python, resolve_repo_root, PROJECT_ROOT_ENV};
    use crate::molecule::stata_path_resolution::resolve_windows_stata_path_with_prompt;
    use clap::Parser;
    use std::fs;
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn make_repo(dir: &Path) {
        fs::create_dir_all(dir.join("src").join("stata_cli").join("entry")).unwrap();
        fs::write(
            dir.join("pyproject.toml"),
            "[project]\nname = 'stata-cli'\n",
        )
        .unwrap();
        fs::write(
            dir.join("src")
                .join("stata_cli")
                .join("entry")
                .join("backend_main.py"),
            "print('ok')\n",
        )
        .unwrap();
    }

    fn write_mock_python(path: &Path) {
        if cfg!(windows) {
            fs::write(
                path,
                "@echo off\r\nif \"%1\"==\"-c\" (\r\n  echo 3.11\r\n) else (\r\n  echo Python 3.11.0\r\n)\r\n",
            )
            .unwrap();
        } else {
            fs::write(
                path,
                "#!/bin/sh\nif [ \"$1\" = \"-c\" ]; then\n  echo 3.11\nelse\n  echo Python 3.11.0\nfi\n",
            )
            .unwrap();
            #[cfg(unix)]
            {
                let mut perms = fs::metadata(path).unwrap().permissions();
                perms.set_mode(0o755);
                fs::set_permissions(path, perms).unwrap();
            }
        }
    }

    fn windows_like_cli() -> Cli {
        Cli::parse_from(["stata-cli", "doctor"])
    }

    #[test]
    fn parse_run_command() {
        let cli = Cli::parse_from(["stata-cli", "run", "--code", "display 1+1"]);
        match cli.command {
            Commands::Run { code } => assert_eq!(code, "display 1+1"),
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn run_command_keeps_working_dir_flag() {
        let cli = Cli::parse_from([
            "stata-cli",
            "--working-dir",
            "/tmp",
            "run",
            "--code",
            "display 1+1",
        ]);
        assert_eq!(cli.working_dir, Some(PathBuf::from("/tmp")));
        match cli.command {
            Commands::Run { code } => assert_eq!(code, "display 1+1"),
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn run_command_rejects_removed_timeout_flag() {
        let error = Cli::try_parse_from([
            "stata-cli",
            "--timeout",
            "17",
            "run",
            "--code",
            "display 1+1",
        ])
        .unwrap_err()
        .to_string();
        assert!(error.contains("--timeout"));
    }

    #[test]
    fn deprecated_json_flag_still_parses() {
        let cli = Cli::parse_from(["stata-cli", "--json", "doctor"]);
        assert!(cli.json);
    }

    #[test]
    fn parse_doctor_command() {
        let cli = Cli::parse_from(["stata-cli", "doctor"]);
        match cli.command {
            Commands::Doctor => {}
            _ => panic!("expected doctor command"),
        }
    }

    #[test]
    fn parse_init_command() {
        let cli = Cli::parse_from(["stata-cli", "init"]);
        match cli.command {
            Commands::Init => {}
            _ => panic!("expected init command"),
        }
    }

    #[test]
    fn parse_data_export_command() {
        let temp = std::env::temp_dir();
        let output_path = temp.join("out.csv");
        let input_path = temp.join("input.dta");
        let cli = Cli::parse_from([
            "stata-cli",
            "data",
            "export-csv",
            "--output",
            output_path.to_string_lossy().as_ref(),
            "--input-dta",
            input_path.to_string_lossy().as_ref(),
            "--replace",
        ]);

        match cli.command {
            Commands::Data {
                command:
                    DataCommands::ExportCsv {
                        output,
                        input_dta,
                        replace,
                        ..
                    },
            } => {
                assert_eq!(output, output_path);
                assert_eq!(input_dta, input_path);
                assert!(replace);
            }
            _ => panic!("expected data export-csv command"),
        }
    }

    #[test]
    fn parse_file_command_with_globals() {
        let cli = Cli::parse_from([
            "stata-cli",
            "--stata-path",
            "/Applications/Stata",
            "file",
            "tests/fixtures/test_stata.do",
            "--working-dir",
            "/tmp",
        ]);

        assert_eq!(cli.stata_path.as_deref(), Some("/Applications/Stata"));
        match cli.command {
            Commands::File {
                path, working_dir, ..
            } => {
                assert_eq!(path, PathBuf::from("tests/fixtures/test_stata.do"));
                assert_eq!(working_dir, Some(PathBuf::from("/tmp")));
            }
            _ => panic!("expected file command"),
        }
    }

    #[test]
    fn parse_data_view_uses_agent_friendly_default() {
        let cli = Cli::parse_from([
            "stata-cli",
            "data",
            "view",
            "--input-dta",
            "scene/grilic.dta",
        ]);
        match cli.command {
            Commands::Data {
                command:
                    DataCommands::View {
                        max_rows,
                        input_dta,
                        ..
                    },
            } => {
                assert_eq!(max_rows, 50);
                assert_eq!(input_dta, PathBuf::from("scene/grilic.dta"));
            }
            _ => panic!("expected data view command"),
        }
    }

    #[test]
    fn base_backend_cli_args_use_module_entrypoint() {
        let cli = Cli::parse_from([
            "stata-cli",
            "--stata-path",
            "/Applications/Stata",
            "--log-level",
            "INFO",
            "doctor",
        ]);

        let args = base_backend_args(&cli, true, true);
        assert!(args.iter().any(|arg| arg == "--stata-path"));
        assert!(args.iter().any(|arg| arg == "--json"));
        assert!(args.iter().any(|arg| arg == "-m"));
        assert!(args.iter().any(|arg| arg == "stata_cli.entry.backend_main"));
        assert!(!args
            .iter()
            .any(|arg| arg.to_string_lossy().contains("stata_cli_backend.py")));
    }

    #[test]
    fn discover_repo_root_from_ancestor_directory() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("stata-cli");
        make_repo(&repo);
        let nested = repo.join("rust-cli").join("src");
        fs::create_dir_all(&nested).unwrap();

        let discovered = discover_repo_root_from(&nested).unwrap();
        assert_eq!(discovered, repo);
    }

    #[test]
    fn executable_path_inside_repo_bin_discovers_repo_root() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("stata-cli");
        make_repo(&repo);
        let bin = repo.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let fake_exe = if cfg!(windows) {
            bin.join("stata-cli.exe")
        } else {
            bin.join("stata-cli")
        };
        fs::write(&fake_exe, "placeholder").unwrap();

        let discovered = normalize_repo_root(&fake_exe).unwrap();
        assert_eq!(discovered, fs::canonicalize(repo).unwrap());
    }

    #[test]
    fn load_cli_config_reads_project_root_and_stata_path() {
        let temp = tempdir().unwrap();
        let config_path = temp.path().join("config.toml");
        let project_root = temp.path().join("project");
        let stata_path = temp.path().join("Stata18");
        fs::write(
            &config_path,
            format!(
                "project_root = {:?}\nstata_path = {:?}\n",
                project_root.to_string_lossy(),
                stata_path.to_string_lossy()
            ),
        )
        .unwrap();

        let config = load_cli_config(&config_path).unwrap().unwrap();
        assert_eq!(config.project_root, Some(project_root));
        assert_eq!(config.stata_path, Some(stata_path));
    }

    #[test]
    fn write_cli_config_preserves_fields() {
        let temp = tempdir().unwrap();
        let config_path = temp.path().join("config.toml");
        let config = CliConfig {
            project_root: Some(temp.path().join("repo")),
            stata_path: Some(temp.path().join("Stata18")),
        };

        write_cli_config(&config_path, &config).unwrap();
        let reloaded = load_cli_config(&config_path).unwrap().unwrap();
        assert_eq!(reloaded.project_root, config.project_root);
        assert_eq!(reloaded.stata_path, config.stata_path);
    }

    #[test]
    fn repo_root_override_wins() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        make_repo(&repo);

        std::env::set_var(PROJECT_ROOT_ENV, &repo);
        let resolved = resolve_repo_root().unwrap();
        std::env::remove_var(PROJECT_ROOT_ENV);

        assert_eq!(resolved.path, fs::canonicalize(repo).unwrap());
        assert_eq!(resolved.source, "environment");
    }

    #[test]
    fn inspect_python_version_accepts_mock_interpreter() {
        let dir = tempdir().unwrap();
        let script = if cfg!(windows) {
            dir.path().join("python311-mock.cmd")
        } else {
            dir.path().join("python311-mock")
        };

        write_mock_python(&script);

        assert_eq!(inspect_python_version(&script).unwrap(), "3.11");
    }

    #[test]
    fn resolve_python_uses_uv_managed_environment() {
        if cfg!(windows) {
            let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("rust-cli should live under the repo root");
            let expected = project_python_for_tests(repo);
            if !expected.exists() {
                return;
            }

            let resolved = resolve_python(None, repo).unwrap();
            assert_eq!(resolved.path, expected);
            assert_eq!(resolved.source, "project .venv");
            assert_eq!(resolved.version, "3.11");
            return;
        }

        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        make_repo(&repo);
        let expected = project_python_for_tests(&repo);
        let parent = expected.parent().unwrap();
        fs::create_dir_all(parent).unwrap();
        write_mock_python(&expected);

        let resolved = resolve_python(None, &repo).unwrap();
        assert_eq!(resolved.path, expected);
        assert_eq!(resolved.source, "project .venv");
        assert_eq!(resolved.version, "3.11");
    }

    #[test]
    fn resolve_python_errors_when_uv_environment_is_missing() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        make_repo(&repo);

        let error = resolve_python(None, &repo).unwrap_err().to_string();
        assert!(error.contains("uv sync --all-extras --python 3.11"));
    }

    #[test]
    fn session_args_absolutizes_working_dir() {
        let cli = Cli::parse_from(["stata-cli", "--working-dir", ".", "doctor"]);

        let args = session_args(&cli);
        let working_dir = args
            .windows(2)
            .find(|pair| pair[0] == "--working-dir")
            .map(|pair| PathBuf::from(&pair[1]))
            .unwrap();

        assert_eq!(working_dir, std::env::current_dir().unwrap());
    }

    #[test]
    fn data_backend_invocation_resolves_relative_output_against_working_dir() {
        let cwd = std::env::current_dir().unwrap();
        let command = DataCommands::ExportCsv {
            output: PathBuf::from("export.csv"),
            input_dta: PathBuf::from("scene/grilic.dta"),
            working_dir: Some(PathBuf::from("scene")),
            replace: true,
        };

        let (_, args) = data_backend_invocation(&command).unwrap();
        let rendered: Vec<PathBuf> = args
            .iter()
            .filter(|arg| !arg.to_string_lossy().starts_with("--"))
            .map(PathBuf::from)
            .collect();

        assert!(rendered.contains(&cwd.join("scene").join("export.csv")));
        assert!(rendered.contains(&cwd.join("scene/grilic.dta")));
        assert!(rendered.contains(&cwd.join("scene")));
    }

    #[test]
    fn absolutize_cli_path_uses_process_cwd_for_relative_input() {
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(
            absolutize_cli_path(Path::new("scene/smoke_test.do")).unwrap(),
            cwd.join("scene/smoke_test.do")
        );
    }

    #[test]
    fn windows_config_path_uses_appdata() {
        if !cfg!(windows) {
            return;
        }
        let temp = tempdir().unwrap();
        std::env::set_var("APPDATA", temp.path());
        let path = default_config_path().unwrap();
        std::env::remove_var("APPDATA");
        assert_eq!(path, temp.path().join("stata-cli").join("config.toml"));
    }

    #[test]
    fn resolve_windows_stata_path_prefers_cli_flag() {
        if !cfg!(windows) {
            return;
        }
        let temp = tempdir().unwrap();
        let stata_path = temp.path().join("Stata18");
        fs::create_dir_all(&stata_path).unwrap();
        let cli = Cli::parse_from([
            "stata-cli",
            "--stata-path",
            stata_path.to_string_lossy().as_ref(),
            "doctor",
        ]);

        let resolved = resolve_windows_stata_path_with_prompt(&cli, None, false, |_| {
            panic!("should not prompt")
        })
        .unwrap();

        assert_eq!(resolved.path, Some(stata_path));
        assert_eq!(resolved.source, Some(StataPathSource::CliFlag));
        assert!(!resolved.save_to_config);
    }

    #[test]
    fn resolve_windows_stata_path_uses_saved_config() {
        if !cfg!(windows) {
            return;
        }
        let temp = tempdir().unwrap();
        let stata_path = temp.path().join("Stata18");
        fs::create_dir_all(&stata_path).unwrap();
        let config = CliConfig {
            project_root: None,
            stata_path: Some(stata_path.clone()),
        };

        let resolved = resolve_windows_stata_path_with_prompt(
            &windows_like_cli(),
            Some(&config),
            false,
            |_| panic!("should not prompt"),
        )
        .unwrap();

        assert_eq!(resolved.path, Some(stata_path));
        assert_eq!(resolved.source, Some(StataPathSource::Config));
        assert!(!resolved.save_to_config);
    }

    #[test]
    fn resolve_windows_stata_path_errors_non_interactive_when_saved_path_missing() {
        if !cfg!(windows) {
            return;
        }
        let temp = tempdir().unwrap();
        let config = CliConfig {
            project_root: None,
            stata_path: Some(temp.path().join("MissingStata")),
        };
        let error = resolve_windows_stata_path_with_prompt(
            &windows_like_cli(),
            Some(&config),
            false,
            |_| panic!("should not prompt"),
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("Update"));
    }

    #[test]
    fn resolve_windows_stata_path_prompts_and_marks_for_save() {
        if !cfg!(windows) {
            return;
        }
        let temp = tempdir().unwrap();
        let saved_invalid = temp.path().join("MissingStata");
        let prompted = temp.path().join("PromptedStata");
        fs::create_dir_all(&prompted).unwrap();
        let config = CliConfig {
            project_root: None,
            stata_path: Some(saved_invalid),
        };
        let mut prompt_count = 0usize;

        let resolved = resolve_windows_stata_path_with_prompt(
            &windows_like_cli(),
            Some(&config),
            true,
            |_| {
                prompt_count += 1;
                Ok(Some(prompted.clone()))
            },
        )
        .unwrap();

        assert_eq!(prompt_count, 1);
        assert_eq!(resolved.path, Some(prompted));
        assert_eq!(resolved.source, Some(StataPathSource::Prompt));
        assert!(resolved.save_to_config);
    }
}
