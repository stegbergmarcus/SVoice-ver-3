use std::path::PathBuf;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

use crate::protocol::{SttRequest, SttResponse};

#[derive(Debug, thiserror::Error)]
pub enum SidecarError {
    #[error("kunde inte spawna Python-sidecar: {0}")]
    Spawn(String),
    #[error("sidecar stängde oväntat")]
    Closed,
    #[error("protokoll-fel: {0}")]
    Protocol(String),
    #[error("IO-fel: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON-fel: {0}")]
    Json(#[from] serde_json::Error),
}

pub struct Sidecar {
    child: Child,
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<BufReader<ChildStdout>>,
}

impl Sidecar {
    pub async fn spawn(python_path: &PathBuf, script_path: &PathBuf) -> Result<Self, SidecarError> {
        let mut child = Command::new(python_path)
            .arg(script_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| SidecarError::Spawn(e.to_string()))?;

        let stdin = child.stdin.take().ok_or(SidecarError::Closed)?;
        let stdout = BufReader::new(child.stdout.take().ok_or(SidecarError::Closed)?);
        let this = Self {
            child,
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(stdout),
        };

        // Vänta på ready-svar
        match this.read_response().await? {
            SttResponse::Ready => Ok(this),
            other => Err(SidecarError::Protocol(format!("förväntade Ready, fick {other:?}"))),
        }
    }

    pub async fn send_request(&self, req: &SttRequest) -> Result<(), SidecarError> {
        let mut stdin = self.stdin.lock().await;
        let line = serde_json::to_string(req)? + "\n";
        stdin.write_all(line.as_bytes()).await?;
        stdin.flush().await?;
        Ok(())
    }

    pub async fn send_audio(&self, samples: &[f32]) -> Result<(), SidecarError> {
        let mut stdin = self.stdin.lock().await;
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        stdin.write_all(&bytes).await?;
        stdin.flush().await?;
        Ok(())
    }

    pub async fn read_response(&self) -> Result<SttResponse, SidecarError> {
        let mut stdout = self.stdout.lock().await;
        let mut line = String::new();
        let n = stdout.read_line(&mut line).await?;
        if n == 0 {
            return Err(SidecarError::Closed);
        }
        let resp: SttResponse = serde_json::from_str(line.trim())?;
        Ok(resp)
    }

    pub async fn shutdown(mut self) -> Result<(), SidecarError> {
        let _ = self.send_request(&SttRequest::Shutdown).await;
        let _ = self.child.wait().await;
        Ok(())
    }
}
