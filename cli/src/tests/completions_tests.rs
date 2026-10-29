use clap_complete::Shell;

use super::write_script;

fn render(shell: Shell) -> String {
    let mut buf = Vec::new();
    write_script(shell, &mut buf);
    String::from_utf8(buf).expect("completion scripts must be UTF-8")
}

#[test]
fn every_supported_shell_gets_a_script() {
    for shell in [
        Shell::Bash,
        Shell::Elvish,
        Shell::Fish,
        Shell::PowerShell,
        Shell::Zsh,
    ] {
        let script = render(shell);
        assert!(
            !script.trim().is_empty(),
            "{shell:?} completion script must not be empty"
        );
        assert!(
            script.contains("super"),
            "{shell:?} completion script must reference the binary name"
        );
        assert!(
            script.contains("restart"),
            "{shell:?} completion script must include subcommands"
        );
    }
}

#[test]
fn scripts_include_flags_and_aliases() {
    let bash = render(Shell::Bash);
    assert!(
        bash.contains("--dry-run"),
        "bash script must complete flags"
    );
    assert!(
        bash.contains("completions"),
        "bash script must complete the completions command"
    );
    for alias in ["ls", "rs", "rm"] {
        assert!(
            bash.contains(alias),
            "bash script must complete alias {alias:?}"
        );
    }
}
