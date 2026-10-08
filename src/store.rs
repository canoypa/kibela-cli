const SERVICE: &str = "kibela-cli";

pub use backend::{delete, get, set};

// The keychain grants access to the program that reads an item. Going through
// /usr/bin/security keeps that program the same Apple tool, so rebuilding kibela does not
// bring up the keychain prompt again.
#[cfg(target_os = "macos")]
mod backend {
    use std::{
        io::Write,
        process::{Command, Output, Stdio},
    };

    use super::SERVICE;

    const SECURITY: &str = "/usr/bin/security";
    const ITEM_NOT_FOUND: i32 = 44;

    pub fn get(team: &str) -> Result<Option<String>, String> {
        let output = run(
            &["find-generic-password", "-s", SERVICE, "-a", team, "-w"],
            None,
        )?;
        match output.status.code() {
            Some(0) => Ok(Some(
                String::from_utf8_lossy(&output.stdout)
                    .trim_end_matches('\n')
                    .to_string(),
            )),
            Some(ITEM_NOT_FOUND) => Ok(None),
            _ => Err(failure(&output)),
        }
    }

    pub fn set(team: &str, token: &str) -> Result<(), String> {
        // An item created by another program keeps that program's access list, so replace it.
        delete(team)?;
        // Passed on standard input and as hex, so the token stays out of the process list and
        // needs no quoting.
        let command = format!(
            "add-generic-password -s {SERVICE} -a {team} -X {}\n",
            hex(token)
        );
        let output = run(&["-i"], Some(&command))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(failure(&output))
        }
    }

    pub fn delete(team: &str) -> Result<(), String> {
        let output = run(
            &["delete-generic-password", "-s", SERVICE, "-a", team],
            None,
        )?;
        match output.status.code() {
            Some(0 | ITEM_NOT_FOUND) => Ok(()),
            _ => Err(failure(&output)),
        }
    }

    fn run(args: &[&str], input: Option<&str>) -> Result<Output, String> {
        let mut child = Command::new(SECURITY)
            .args(args)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot run {SECURITY}: {e}"))?;
        if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
            stdin
                .write_all(input.as_bytes())
                .map_err(|e| format!("cannot write to {SECURITY}: {e}"))?;
        }
        child
            .wait_with_output()
            .map_err(|e| format!("cannot run {SECURITY}: {e}"))
    }

    fn failure(output: &Output) -> String {
        match String::from_utf8_lossy(&output.stderr).trim() {
            "" => format!("{SECURITY} {}", output.status),
            message => message.to_string(),
        }
    }

    fn hex(text: &str) -> String {
        text.bytes().map(|b| format!("{b:02x}")).collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn hex_encodes_each_byte() {
            assert_eq!(hex("a/Z-9"), "612f5a2d39");
            assert_eq!(hex("é"), "c3a9");
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod backend {
    use keyring::Entry;

    use super::SERVICE;

    fn entry(team: &str) -> Result<Entry, String> {
        Entry::new(SERVICE, team).map_err(|e| e.to_string())
    }

    pub fn get(team: &str) -> Result<Option<String>, String> {
        match entry(team)?.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn set(team: &str, token: &str) -> Result<(), String> {
        entry(team)?.set_password(token).map_err(|e| e.to_string())
    }

    pub fn delete(team: &str) -> Result<(), String> {
        match entry(team)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}
