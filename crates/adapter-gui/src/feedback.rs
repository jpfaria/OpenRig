use anyhow::{Context, Result};
use std::process::Command;

/// Builds the GitHub issue body from description and optional error context.
pub fn build_issue_body(description: &str, context: &str) -> String {
    let mut body = format!("**Descrição**\n{description}");
    if !context.is_empty() {
        body.push_str(&format!("\n\n**Contexto do erro**\n```\n{context}\n```"));
    }
    body.push_str("\n\n---\n*Reportado pela interface do OpenRig*");
    body
}

/// Submits a feedback issue via the `gh` CLI.
///
/// `kind` must be `"bug"` or `"enhancement"`.
pub fn submit_gh_feedback(kind: &str, title: &str, description: &str, context: &str) -> Result<()> {
    let body = build_issue_body(description, context);
    let label = if kind == "bug" { "bug" } else { "enhancement" };
    let status = Command::new("gh")
        .args(["issue", "create", "--title", title, "--body", &body, "--label", label])
        .status()
        .context("Falha ao executar 'gh'. Certifique-se de que o GitHub CLI está instalado e autenticado.")?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "gh issue create falhou com código {}",
            status.code().unwrap_or(-1)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_body_includes_description() {
        let body = build_issue_body("Minha sugestão de melhoria", "");
        assert!(body.contains("Minha sugestão de melhoria"));
    }

    #[test]
    fn issue_body_includes_context_when_provided() {
        let body = build_issue_body("Bug encontrado", "Erro: falha ao processar bloco");
        assert!(body.contains("Erro: falha ao processar bloco"));
    }

    #[test]
    fn issue_body_omits_context_section_when_empty() {
        let body = build_issue_body("Sugestão simples", "");
        assert!(!body.contains("Contexto do erro"));
    }

    #[test]
    fn issue_body_includes_footer() {
        let body = build_issue_body("Algo", "");
        assert!(body.contains("OpenRig"));
    }

    #[test]
    fn issue_body_wraps_context_in_code_block() {
        let body = build_issue_body("Bug", "stack trace aqui");
        assert!(body.contains("```\nstack trace aqui\n```"));
    }
}
