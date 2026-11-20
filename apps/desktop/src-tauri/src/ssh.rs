use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SshTarget {
    pub host: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
    pub config_file: Option<String>,
}

pub fn quote(raw: &str) -> String {
    format!("'{}'", raw.replace('\'', "'\\''"))
}

impl SshTarget {
    pub fn args(
        &self,
        interactive: bool,
        cwd: &str,
        command: Option<&str>,
    ) -> Result<Vec<String>, String> {
        if self.host.is_empty()
            || self.host.starts_with('-')
            || !self
                .host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || ".-_:[]".contains(c))
        {
            return Err("Enter an SSH hostname, IP address, or SSH config alias".into());
        }
        let user = self.user.as_deref().unwrap_or("").trim();
        if !user
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
            || user.starts_with('-')
        {
            return Err("Invalid SSH username".into());
        }
        if self.port == Some(0) {
            return Err("SSH port must be between 1 and 65535".into());
        }
        let mut args = vec![
            if interactive { "-tt" } else { "-T" }.into(),
            "-o".into(),
            "ConnectTimeout=15".into(),
            "-o".into(),
            "ServerAliveInterval=30".into(),
            "-o".into(),
            "ServerAliveCountMax=3".into(),
        ];
        if let Some(path) = self
            .config_file
            .as_deref()
            .filter(|path| !path.trim().is_empty())
        {
            let path = crate::workspace::expand_dir(path);
            if !path.is_file() {
                return Err("The SSH config file does not exist on this computer".into());
            }
            args.extend(["-F".into(), path.to_string_lossy().into_owned()]);
        }
        if !interactive {
            args.extend(["-o".into(), "BatchMode=yes".into()]);
        }
        if let Some(port) = self.port {
            args.extend(["-p".into(), port.to_string()]);
        }
        if let Some(path) = self
            .identity_file
            .as_deref()
            .filter(|p| !p.trim().is_empty())
        {
            let path = crate::workspace::expand_dir(path);
            if !path.is_file() {
                return Err("The SSH identity file does not exist on this computer".into());
            }
            args.extend(["-i".into(), path.to_string_lossy().into_owned()]);
        }
        let target = if user.is_empty() {
            self.host.clone()
        } else {
            format!("{user}@{}", self.host)
        };
        args.extend(["--".into(), target]);
        let directory = match cwd.trim() {
            "" | "~" => "\"$HOME\"".to_owned(),
            value if value.starts_with("~/") => format!("\"$HOME\"/{}", quote(&value[2..])),
            value => quote(value),
        };
        let script = format!(
            "cd -- {directory} && {}",
            command.unwrap_or("exec \"${SHELL:-/bin/sh}\" -l")
        );
        args.push(format!("exec sh -lc {}", quote(&script)));
        Ok(args)
    }
}

pub async fn exec(
    target: &SshTarget,
    cwd: &str,
    command: &str,
) -> Result<std::process::Output, String> {
    let args = target.args(false, cwd, Some(command))?;
    tokio::time::timeout(
        Duration::from_secs(30),
        tokio::process::Command::new("ssh")
            .args(args)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| "SSH command timed out. Check your connection and SSH agent.".to_string())?
    .map_err(|e| format!("Could not start SSH: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ssh_arguments_never_interpret_user_values_as_options_or_commands() {
        let target = SshTarget {
            host: "my-server".into(),
            user: Some("agent".into()),
            port: Some(2222),
            identity_file: None,
            config_file: None,
        };
        let cwd = "/tmp/project's $(touch /tmp/must-not-exist)";
        let args = target.args(true, cwd, None).unwrap();
        assert_eq!(
            &args[args.len() - 3..args.len() - 1],
            &["--", "agent@my-server"]
        );
        assert!(args.last().unwrap().contains("cd --"));
        assert!(!args.contains(&"StrictHostKeyChecking=no".to_owned()));
        let mut bad = target.clone();
        bad.host = "-oProxyCommand=bad".into();
        assert!(bad.args(true, "", None).is_err());
        bad.host = "host;bad".into();
        assert!(bad.args(true, "", None).is_err());
        bad = target;
        bad.user = Some("user@host".into());
        assert!(bad.args(true, "", None).is_err());
    }
}
