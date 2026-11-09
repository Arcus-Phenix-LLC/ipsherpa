use std::process::ExitCode;

fn server_argument(args: impl IntoIterator<Item = String>) -> Result<Option<String>, String> {
    let args: Vec<_> = args.into_iter().collect();
    match args.as_slice() {
        [help] if help == "--help" || help == "-h" => Ok(None),
        [command, flag, server]
            if command == "scopes"
                && flag == "--server"
                && !server.trim().is_empty()
                && !server.contains('\0')
                && !server.starts_with('-') =>
        {
            Ok(Some(server.clone()))
        }
        _ => Err("Usage: ipsherpa scopes --server <hostname-or-IP>".into()),
    }
}

fn run() -> Result<(), String> {
    let Some(server) = server_argument(std::env::args().skip(1))? else {
        println!("Usage: ipsherpa scopes --server <hostname-or-IP>\nRead-only scope IDs; uses the current Windows identity.");
        return Ok(());
    };
    #[cfg(windows)]
    {
        let scopes = ipsherpa::list_scopes(&server)?;
        println!("DHCP scopes on {server}: {}", scopes.len());
        for scope in scopes {
            println!("{scope}");
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = server;
        Err("Native DHCP enumeration requires Windows.".into())
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(args: &[&str]) -> Result<Option<String>, String> {
        server_argument(args.iter().map(|s| s.to_string()))
    }
    #[test]
    fn requires_explicit_target_and_known_command() {
        assert_eq!(
            parse(&["scopes", "--server", "DHCP01"]).unwrap(),
            Some("DHCP01".into())
        );
        for args in [
            vec![],
            vec!["scopes"],
            vec!["delete", "--server", "DHCP01"],
            vec!["scopes", "--server", ""],
            vec!["scopes", "--server", "a\0b"],
        ] {
            assert!(parse(&args).is_err());
        }
        assert_eq!(parse(&["--help"]).unwrap(), None);
    }
}
