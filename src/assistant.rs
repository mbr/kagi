//! Assistant subprocess execution for answering questions about pages.

use std::{io, process::Stdio};

use thiserror::Error;
use tokio::{io::AsyncWriteExt, process::Command};

/// Executable invoked to answer questions.
const ASSISTANT_COMMAND: &str = "claude";

/// Errors raised while running the assistant.
#[derive(Debug, Error)]
pub enum AssistantError {
    /// The assistant executable could not be started.
    #[error("failed to run `{ASSISTANT_COMMAND}`: {source}")]
    Spawn {
        /// Underlying process spawn failure.
        #[source]
        source: io::Error,
    },

    /// The assistant did not expose a standard input pipe.
    #[error("`{ASSISTANT_COMMAND}` provided no standard input pipe")]
    MissingStdin,

    /// The prompt could not be written to the assistant.
    #[error("failed to send prompt to `{ASSISTANT_COMMAND}`: {source}")]
    Write {
        /// Underlying write failure.
        #[source]
        source: io::Error,
    },

    /// The assistant could not be awaited.
    #[error("failed to await `{ASSISTANT_COMMAND}`: {source}")]
    Wait {
        /// Underlying wait failure.
        #[source]
        source: io::Error,
    },

    /// The assistant exited unsuccessfully.
    #[error("`{ASSISTANT_COMMAND}` failed: {stderr}")]
    Failed {
        /// Standard error output produced by the assistant.
        stderr: String,
    },
}

/// Answers a question about an extracted document.
///
/// The document is passed on standard input rather than as an argument so
/// that large pages do not exceed the operating system argument limit.
pub async fn answer(document: &str, question: &str) -> Result<String, AssistantError> {
    let mut command = Command::new(ASSISTANT_COMMAND);
    command
        .arg("--print")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = command
        .spawn()
        .map_err(|source| AssistantError::Spawn { source })?;
    let mut stdin = child.stdin.take().ok_or(AssistantError::MissingStdin)?;
    let prompt = prompt(document, question);

    // Writing and draining must overlap: a document large enough to fill the
    // stdout pipe would otherwise deadlock the child against our write. The
    // pipe must also be dropped rather than just shut down, because only the
    // drop closes the descriptor and lets the child see end of input.
    let write = async move {
        stdin.write_all(prompt.as_bytes()).await?;
        stdin.shutdown().await?;
        drop(stdin);
        Ok::<(), io::Error>(())
    };
    let (written, output) = tokio::join!(write, child.wait_with_output());

    written.map_err(|source| AssistantError::Write { source })?;
    let output = output.map_err(|source| AssistantError::Wait { source })?;
    if !output.status.success() {
        return Err(AssistantError::Failed {
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Builds the prompt instructing the assistant to stay within the document.
fn prompt(document: &str, question: &str) -> String {
    format!(
        "Below is the content of one or more web pages, extracted and \
         converted to Markdown.\n\n\
         <document>\n{document}\n</document>\n\n\
         Answer this question about the document:\n\n\
         <question>\n{question}\n</question>\n\n\
         Base the answer only on the document. Reproduce figures, quotes and \
         code exactly as they appear rather than paraphrasing them. If the \
         document does not contain the answer, say so plainly instead of \
         guessing or falling back on prior knowledge.\n"
    )
}

#[cfg(test)]
mod tests {
    use crate::assistant::prompt;

    #[test]
    fn prompt_contains_document_and_question() {
        let prompt = prompt("extracted body", "what is it?");
        assert!(prompt.contains("<document>\nextracted body\n</document>"));
        assert!(prompt.contains("<question>\nwhat is it?\n</question>"));
    }
}
