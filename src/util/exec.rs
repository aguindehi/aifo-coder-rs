use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use wait_timeout::ChildExt;

/// Structured command execution with timeouts and environment filtering.
#[derive(Debug, Clone)]
pub struct ExecService {
    allowed_env: Option<BTreeSet<String>>,
    default_timeout: Duration,
}

impl ExecService {
    pub fn new(default_timeout: Duration) -> Self {
        Self {
            allowed_env: None,
            default_timeout,
        }
    }

    pub fn with_allowed_env<I, S>(default_timeout: Duration, allowed: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let set = allowed.into_iter().map(Into::into).collect();
        Self {
            allowed_env: Some(set),
            default_timeout,
        }
    }

    pub fn run(&self, request: ExecRequest) -> Result<ExecOutput> {
        let mut cmd = Command::new(&request.program);
        for arg in &request.args {
            cmd.arg(arg);
        }
        if let Some(ref cwd) = request.cwd {
            cmd.current_dir(cwd);
        }

        if !request.inherit_env || self.allowed_env.is_some() {
            cmd.env_clear();
        }

        if let Some(allowed) = &self.allowed_env {
            for (key, value) in std::env::vars_os() {
                if let Ok(k) = key.clone().into_string() {
                    if allowed.contains(&k) {
                        cmd.env(&key, &value);
                    }
                }
            }
        } else if request.inherit_env {
            for (key, value) in std::env::vars_os() {
                cmd.env(&key, &value);
            }
        }

        for (key, value) in request.env {
            cmd.env(&key, &value);
        }

        if request.capture_output {
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        }

        let mut child = cmd.spawn().with_context(|| {
            format!(
                "failed to spawn {:?} with args {:?}",
                request.program, request.args
            )
        })?;

        let mut stdout_pipe = if request.capture_output {
            child.stdout.take()
        } else {
            None
        };
        let mut stderr_pipe = if request.capture_output {
            child.stderr.take()
        } else {
            None
        };

        let timeout = request.timeout.unwrap_or(self.default_timeout);
        let started = Instant::now();
        let status = if timeout.is_zero() {
            child.wait().context("failed to wait for process")?
        } else {
            match child
                .wait_timeout(timeout)
                .context("failed to wait with timeout")?
            {
                Some(status) => status,
                None => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(anyhow!(
                        "command {:?} timed out after {:?}",
                        request.program,
                        timeout
                    ));
                }
            }
        };

        let duration = started.elapsed();
        let (stdout, stderr) = if request.capture_output {
            let stdout = read_stream(stdout_pipe.as_mut())?;
            let stderr = read_stream(stderr_pipe.as_mut())?;
            (stdout, stderr)
        } else {
            (String::new(), String::new())
        };

        Ok(ExecOutput {
            status,
            duration,
            stdout,
            stderr,
        })
    }
}

fn read_stream(stream: Option<&mut impl io::Read>) -> Result<String> {
    let mut buf = String::new();
    if let Some(reader) = stream {
        reader
            .read_to_string(&mut buf)
            .context("failed to read process output")?;
    }
    Ok(buf)
}

impl Default for ExecService {
    fn default() -> Self {
        Self::new(Duration::from_secs(30))
    }
}

#[derive(Debug, Default)]
pub struct ExecRequest {
    program: OsString,
    args: Vec<OsString>,
    cwd: Option<PathBuf>,
    env: Vec<(OsString, OsString)>,
    inherit_env: bool,
    timeout: Option<Duration>,
    capture_output: bool,
}

impl ExecRequest {
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            inherit_env: false,
            ..Self::default()
        }
    }

    pub fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn cwd(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cwd = Some(dir.into());
        self
    }

    pub fn env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    pub fn inherit_env(mut self, inherit: bool) -> Self {
        self.inherit_env = inherit;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn capture_output(mut self, capture: bool) -> Self {
        self.capture_output = capture;
        self
    }
}

#[derive(Debug)]
pub struct ExecOutput {
    pub status: std::process::ExitStatus,
    pub duration: Duration,
    pub stdout: String,
    pub stderr: String,
}
