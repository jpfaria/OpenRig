use anyhow::{Context, Result};

const GITHUB_TOKEN: &str = match option_env!("OPENRIG_FEEDBACK_TOKEN") {
    Some(t) => t,
    None => "",
};
const GITHUB_REPO: &str = "jpfaria/OpenRig";

/// Builds the GitHub issue body from description and optional error context.
pub fn build_issue_body(description: &str, context: &str) -> String {
    let mut body = format!("**Descrição**\n{description}");
    if !context.is_empty() {
        body.push_str(&format!("\n\n**Contexto do erro**\n```\n{context}\n```"));
    }
    body.push_str("\n\n---\n*Reportado pela interface do OpenRig*");
    body
}

/// Submits a feedback issue via the GitHub REST API.
///
/// `kind` must be `"bug"` or `"enhancement"`.
pub fn submit_gh_feedback(kind: &str, title: &str, description: &str, context: &str) -> Result<()> {
    if GITHUB_TOKEN.is_empty() {
        return Err(anyhow::anyhow!(
            "Token do GitHub não configurado. Compile com OPENRIG_FEEDBACK_TOKEN=<token>."
        ));
    }

    let body = build_issue_body(description, context);
    let label = if kind == "bug" { "bug" } else { "enhancement" };

    let payload = serde_json::json!({
        "title": title,
        "body": body,
        "labels": [label]
    });

    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/issues");

    let response = ureq::post(&url)
        .set("Authorization", &format!("Bearer {GITHUB_TOKEN}"))
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
