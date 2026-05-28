//! Pin the "I need to <verb>" / multilingual deferment-stall detection
//! added 2026-05-28.
//!
//! User report: model emitted
//!   "I need to read the actual code before writing concrete task descriptions."
//! and the self-heal phantom detector did not catch it. Same shape as
//! "let me read" / "I'll read" — pre-action narration with no tool call
//! in the same turn. The fix added "i need to X", "i have to X",
//! "i must X", "i should X" variants to every language's intent_phrases
//! and extended each language's line_start_re to catch them at line
//! start too.

use crate::brain::agent::service::{has_investigative_intent, has_phantom_tool_intent_no_tools};

// ─── English — the literal user report ─────────────────────────────────

#[test]
fn user_reported_phrase_2026_05_28() {
    let text = "I need to read the actual code before writing concrete task descriptions.";
    assert!(
        has_phantom_tool_intent_no_tools(text),
        "verbatim 2026-05-28 user report must trigger the no-tools phantom detector"
    );
    assert!(
        has_investigative_intent(text),
        "must also surface as investigative intent for upstream loop-detection"
    );
}

#[test]
fn english_i_need_to_check_triggers() {
    let text = "I need to check the configuration before changing it.";
    assert!(has_phantom_tool_intent_no_tools(text));
}

#[test]
fn english_i_have_to_verify_triggers() {
    let text = "I have to verify the current state of the database schema first.";
    assert!(has_phantom_tool_intent_no_tools(text));
}

#[test]
fn english_i_must_update_triggers() {
    let text = "I must update the version field before tagging the release.";
    assert!(has_phantom_tool_intent_no_tools(text));
}

#[test]
fn english_i_should_investigate_triggers() {
    let text = "I should investigate the failure logs before suggesting a fix.";
    assert!(has_phantom_tool_intent_no_tools(text));
}

// ─── Multilingual parity ──────────────────────────────────────────────

#[test]
fn spanish_necesito_leer_triggers() {
    // Spanish detection requires ñ / ¿ / ¡ to distinguish from French
    // fallback — include a Spanish-specific character.
    let text = "¿Necesito leer el código real? Sí, primero el año pasado de configuración antes de escribir las descripciones.";
    assert!(
        has_phantom_tool_intent_no_tools(text),
        "Spanish 'necesito leer' must trigger like English 'I need to read'"
    );
}

#[test]
fn portuguese_preciso_ler_triggers() {
    let text = "Preciso ler o código real antes de escrever as descrições concretas das tarefas.";
    assert!(
        has_phantom_tool_intent_no_tools(text),
        "Portuguese 'preciso ler' must trigger"
    );
}

#[test]
fn french_je_dois_lire_triggers() {
    let text = "Je dois lire le vrai code avant d'écrire des descriptions de tâches concrètes.";
    assert!(
        has_phantom_tool_intent_no_tools(text),
        "French 'je dois lire' must trigger"
    );
}

#[test]
fn russian_mne_nuzhno_prochitat_triggers() {
    let text = "Мне нужно прочитать актуальный код, прежде чем писать конкретные описания задач.";
    assert!(
        has_phantom_tool_intent_no_tools(text),
        "Russian 'мне нужно прочитать' must trigger"
    );
}

// ─── Negative cases — must NOT trigger ────────────────────────────────

#[test]
fn legitimate_prose_does_not_trigger() {
    let text = "The configuration is loaded from disk and merged into the runtime state.";
    assert!(
        !has_phantom_tool_intent_no_tools(text),
        "prose without intent phrases must not trigger"
    );
}

#[test]
fn quoted_user_query_about_reading_does_not_trigger() {
    // User asks the agent something; the agent's response shouldn't trigger
    // just because the user's text contained "need to read".
    let text = "Sure, the answer is in the documentation under section 3.2.";
    assert!(!has_phantom_tool_intent_no_tools(text));
}
