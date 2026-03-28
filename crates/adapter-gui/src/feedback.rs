use anyhow::{Context, Result};

// ── Static backend configuration ─────────────────────────────────────────────
//
// Change GITHUB_TOKEN here when rotating credentials.
// To switch backends entirely, replace `GitHubDriver::new()` in
// `FeedbackService::new()` with another driver implementation.
//
// The token needs `Issues: Write` permission on the repository below.
const GITHUB_TOKEN: &str = "";
const GITHUB_REPO: &str = "jpfaria/OpenRig";

// ── Data ─────────────────────────────────────────────────────────────────────

pub struct FeedbackReport {
    pub kind: String,        // "bug" | "enhancement"
    pub title: String,
    pub description: String,
    pub context: String,     // optional error context; empty when not applicable
}

// ── Driver trait ─────────────────────────────────────────────────────────────

/// Backend-agnostic interface for sending feedback.
///
/// To add a new backend (Sentry, a custom HTTP endpoint, etc.), implement this
/// trait and swap `GitHubDriver` in `FeedbackService::new()`.
pub trait FeedbackDriver: Send + Sync {
    fn submit(&self, report: &FeedbackReport) -> Result<()>;
}

// ── GitHub driver ─────────────────────────────────────────────────────────────

pub struct GitHubDriver {
    token: &'static str,
    repo: &'static str,
}

impl GitHubDriver {
    pub fn new() -> Self {
        Self {
            token: GITHUB_TOKEN,
            repo: GITHUB_REPO,
        }
    }

    fn build_body(&self, report: &FeedbackReport) -> String {
        let mut body = format!("**Descrição**\n{}", report.description);
        if !report.context.is_empty() {
            body.push_str(&format!(
                "\n\n**Contexto do erro**\n```\n{}\n```",
                report.context
            ));
        }
        body.push_str("\n\n---\n*Reportado pela interface do OpenRig*");
        body
    }
}

impl FeedbackDriver for GitHubDriver {
    fn submit(&self, report: &FeedbackReport) -> Result<()> {
        if self.token.is_empty() {
            return Err(anyhow::anyhow!("Token do GitHub não configurado."));
        }

        let label = if report.kind == "bug" { "bug" } else { "enhancement" };
        let payload = serde_json::json!({
            "title": report.title,
            "body": self.build_body(report),
            "labels": [label]
        });

        let url = format!("https://api.github.com/repos/{}/issues", self.repo);
        let response = ureq::post(&url)
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Accept", "application/vnd.github+json")
            .set("X-GitHub-Api-Version", "2022-11-28")
            .set("User-Agent", "OpenRig")
            .send_json(payload)
            .context("Falha ao criar issue no GitHub")?;

        if response.status() == 201 {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "GitHub API retornou status {}",
                response.status()
            ))
        }
    }
}

// ── Service ───────────────────────────────────────────────────────────────────

/// Orchestrates feedback submission through the configured driver.
///
/// Callers decide how to handle failures:
/// - **User-triggered**: surface a friendly message when `submit` returns `Err`.
/// - **System auto-report**: call `report_system_error` — logs on failure, no UI noise.
pub struct FeedbackService {
    driver: Box<dyn FeedbackDriver>,
}

impl FeedbackService {
    /// Creates a service with the default driver.
    /// Swap the driver here to change the backend without touching any other code.
    pub fn new() -> Self {
        Self {
            driver: Box::new(GitHubDriver::new()),
        }
    }

    /// Submits a user-triggered feedback report.
    /// Returns `Err` so the caller can show a friendly UI message.
    pub fn submit(&self, report: FeedbackReport) -> Result<()> {
        self.driver.submit(&report)
    }

    /// Auto-reports a system error in a fire-and-forget fashion.
    /// Logs a warning if submission fails; never propagates the error.
    pub fn report_system_error(&self, context: &str, error: &str) {
        let report = FeedbackReport {
            kind: "bug".into(),
            title: format!("[auto] {context}"),
            description: format!("Erro capturado automaticamente em `{context}`."),
            context: error.into(),
        };
        if let Err(e) = self.driver.submit(&report) {
            log::warn!("[feedback] falha ao reportar erro automático ({context}): {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoopDriver;
    impl FeedbackDriver for NoopDriver {
        fn submit(&self, _report: &FeedbackReport) -> Result<()> {
            Ok(())
        }
    }

    fn service_with_noop() -> FeedbackService {
        FeedbackService {
            driver: Box::new(NoopDriver),
        }
    }

    #[test]
    fn submit_delegates_to_driver() {
        let svc = service_with_noop();
        let result = svc.submit(FeedbackReport {
            kind: "bug".into(),
            title: "Test".into(),
            description: "desc".into(),
            context: "".into(),
        });
        assert!(result.is_ok());
    }

    #[test]
    fn report_system_error_does_not_panic() {
        let svc = service_with_noop();
        svc.report_system_error("test_context", "some error");
    }

    #[test]
    fn github_driver_build_body_includes_description() {
        let driver = GitHubDriver::new();
        let report = FeedbackReport {
            kind: "bug".into(),
            title: "T".into(),
            description: "Minha sugestão".into(),
            context: "".into(),
        };
        let body = driver.build_body(&report);
        assert!(body.contains("Minha sugestão"));
    }

    #[test]
    fn github_driver_build_body_includes_context_when_provided() {
        let driver = GitHubDriver::new();
        let report = FeedbackReport {
            kind: "bug".into(),
            title: "T".into(),
            description: "Bug".into(),
            context: "stack trace aqui".into(),
        };
        let body = driver.build_body(&report);
        assert!(body.contains("stack trace aqui"));
        assert!(body.contains("```\nstack trace aqui\n```"));
    }

    #[test]
    fn github_driver_build_body_omits_context_section_when_empty() {
        let driver = GitHubDriver::new();
        let report = FeedbackReport {
            kind: "enhancement".into(),
            title: "T".into(),
            description: "Sugestão".into(),
            context: "".into(),
        };
        let body = driver.build_body(&report);
        assert!(!body.contains("Contexto do erro"));
    }

    #[test]
    fn github_driver_returns_err_when_token_empty() {
        let driver = GitHubDriver {
            token: "",
            repo: "jpfaria/OpenRig",
        };
        let report = FeedbackReport {
            kind: "bug".into(),
            title: "T".into(),
            description: "D".into(),
            context: "".into(),
        };
        assert!(driver.submit(&report).is_err());
    }
}
