//! Static MCP prompts that orient the agent to drive OpenRig via the tools
//! and resources. No business logic lives here.

use rmcp::model::{GetPromptResult, Prompt, PromptMessage, PromptMessageRole};

struct Spec {
    name: &'static str,
    description: &'static str,
    body: &'static str,
}

const SPECS: &[Spec] = &[
    Spec {
        name: "tune_tone",
        description: "Suggest parameter changes for a target tone",
        body: "Read openrig://project, then use the set_block_parameter_* tools to move \
               the chain toward the user's described tone. Explain each change.",
    },
    Spec {
        name: "diagnose_chain",
        description: "Walk the chain and report issues",
        body: "Read openrig://project; report gain-staging, ordering and routing \
               problems and propose concrete tool calls to fix them.",
    },
    Spec {
        name: "build_preset",
        description: "Build a preset from a description",
        body: "Use add_block / set_block_parameter_* / save_project tools to construct \
               a chain matching the description from scratch.",
    },
    Spec {
        name: "analyze_reference",
        description: "Propose a chain from a reference",
        body: "Given a reference description, propose and build a chain with the tools, \
               explaining the signal path.",
    },
];

pub fn prompts() -> Vec<Prompt> {
    SPECS
        .iter()
        .map(|s| Prompt::new(s.name, Some(s.description), None))
        .collect()
}

pub fn get(name: &str) -> Option<GetPromptResult> {
    SPECS.iter().find(|s| s.name == name).map(|s| {
        GetPromptResult::new(vec![PromptMessage::new_text(
            PromptMessageRole::User,
            s.body,
        )])
        .with_description(s.description)
    })
}
