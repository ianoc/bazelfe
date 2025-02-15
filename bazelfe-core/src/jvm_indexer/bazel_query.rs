use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::process::Command;

#[derive(Clone, PartialEq, Debug)]
pub struct ExecuteResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stdout_raw: Vec<u8>,
    pub stderr: String,
    pub stderr_raw: Vec<u8>,
}

impl std::convert::From<std::io::Error> for ExecuteResult {
    fn from(io_e: std::io::Error) -> Self {
        Self {
            exit_code: -120,
            stderr: format!("{:?}", io_e),
            stdout: String::from(""),
            stdout_raw: Vec::default(),
            stderr_raw: Vec::default(),
        }
    }
}
pub type Result<T> = std::result::Result<T, ExecuteResult>;

#[async_trait]
pub trait BazelQuery: std::fmt::Debug + Send + Sync {
    async fn execute(&self, args: &Vec<String>) -> ExecuteResult;
}

#[derive(Clone, Debug)]
pub struct BazelQueryBinaryImpl {
    bazel_executable_path: PathBuf,
}

pub fn from_binary_path(pb: &Path) -> BazelQueryBinaryImpl {
    BazelQueryBinaryImpl {
        bazel_executable_path: pb.to_path_buf(),
    }
}

impl BazelQueryBinaryImpl {
    fn decode_str(data: &Vec<u8>) -> String {
        if !data.is_empty() {
            std::str::from_utf8(data)
                .unwrap_or("Unable to decode content")
                .to_string()
        } else {
            String::from("")
        }
    }
    async fn execute_command(&self, command: &Vec<String>) -> ExecuteResult {
        let mut cmd = Command::new(&self.bazel_executable_path);
        let command_result = match cmd.args(command).output().await {
            Err(e) => return e.into(),
            Ok(o) => o,
        };
        let exit_code = command_result.status.code().unwrap_or(-1);

        ExecuteResult {
            exit_code,
            stdout: BazelQueryBinaryImpl::decode_str(&command_result.stdout),
            stderr: BazelQueryBinaryImpl::decode_str(&command_result.stderr),
            stderr_raw: command_result.stderr,
            stdout_raw: command_result.stdout,
        }
    }
}

#[async_trait]
impl BazelQuery for BazelQueryBinaryImpl {
    async fn execute(&self, args: &Vec<String>) -> ExecuteResult {
        self.execute_command(args).await
    }
}
