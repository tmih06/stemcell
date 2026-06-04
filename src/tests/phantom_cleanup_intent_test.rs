//! Tests for the cleanup / destructive verb intent phrases added
//! 2026-06-04 across all five phantom language TOMLs.
//!
//! Regression context: a turn ran 37 tool calls, then emitted
//! "Good catch. Let me clean up README.md and all the remaining
//! docs plus Rust files. I deleted the template already so all
//! `include_str!` references will break compilation if I don't
//! remove them." and closed. Phantom self-heal didn't fire because
//! `let me clean up` was not in any intent_phrases list. Visible
//! response and the (much longer) thinking content both kept
//! announcing cleanup work that was never dispatched.
//!
//! The five TOMLs got mirrored entries for the same shape:
//!
//!   EN: let me clean up / let me delete / let me remove / let me
//!       refactor / let me reorganize / let me tidy / let me strip
//!       / let me wipe / let me purge / and i'll-X / let's-X / now-X
//!       / i need to X / pronounless need-to X / i have to X / i
//!       must X / i should X variants.
//!   FR: laissez-moi / je vais / allons / j'ai besoin de / je dois /
//!       besoin de (pronounless) + nettoyer / supprimer / enlever /
//!       retirer / refactoriser / réorganiser / ranger / effacer /
//!       purger.
//!   ES: déjame / voy a / vamos a / necesito / tengo que / debo /
//!       debería + limpiar / eliminar / borrar / quitar /
//!       refactorizar / reorganizar / ordenar / purgar.
//!   PT: deixa eu / vou / vamos / preciso / tenho que / devo /
//!       deveria + limpar / deletar / excluir / remover / refatorar
//!       / reorganizar / arrumar / apagar / purgar.
//!   RU: давайте / я / сейчас / теперь / мне нужно / мне надо / я
//!       должен / нужно (pronounless) + очистить / удалить / стереть
//!       / убрать / отрефакторить / реорганизовать / упорядочить /
//!       вычистить.
//!
//! These tests pin both the exact 2026-06-04 leak text AND a
//! cross-language sample so a future TOML edit that drops the
//! cleanup verbs from any one language fails here loudly instead
//! of silently re-opening the gap for users in that locale.

use crate::brain::agent::service::{has_forward_intent_post_success, has_phantom_tool_intent_no_tools};

#[test]
fn english_exact_2026_06_04_leak_text_fires_phantom() {
    // The literal text from the 2026-06-04 screenshot. Must fire
    // both the zero-tool-call detector and the post-success
    // forward-intent detector — the leak happened on a 37-tool turn,
    // so the post-success path is the one that actually mattered.
    let text = "Good catch. Let me clean up README.md and all the remaining docs plus Rust files. \
                I deleted the template already so all `include_str!` references will break \
                compilation if I don't remove them.";
    assert!(
        has_phantom_tool_intent_no_tools(text),
        "the literal 2026-06-04 cleanup leak must fire the standard phantom detector"
    );
    assert!(
        has_forward_intent_post_success(text),
        "the same text must ALSO fire the post-success forward-intent detector \
         since the leak happened on a tool-heavy turn — without this the original \
         exemption disables phantom and the cleanup gets silently dropped"
    );
}

#[test]
fn english_cleanup_verb_variants_each_fire() {
    let leaks = [
        "Let me clean up the duplicated imports across these files.",
        "Let me delete the stale fixtures.",
        "Let me remove the dead helpers.",
        "Let me refactor the auth middleware.",
        "Let me reorganize the test layout.",
        "Let me tidy up the workspace.",
        "Let me strip out the obsolete flags.",
        "Let me wipe the cache directory.",
        "Let me purge the orphaned migrations.",
        "I'll clean up the build artifacts.",
        "Let's refactor the storage layer.",
        "Now delete the abandoned branch.",
        "I need to reorganize this module.",
        "Need to refactor the config loader.",
        "I should clean up the unused exports.",
    ];
    for text in leaks {
        assert!(
            has_phantom_tool_intent_no_tools(text),
            "EN cleanup-verb intent must fire phantom for: {text:?}"
        );
        assert!(
            has_forward_intent_post_success(text),
            "EN cleanup-verb intent must fire post-success forward-intent for: {text:?}"
        );
    }
}

#[test]
fn french_cleanup_verb_variants_each_fire() {
    // Each input MUST carry at least one FR-specific accent marker
    // (é / è / ê / ë / à / â / î / ï / ô / û / ù / ü / ÿ) so
    // `detect_language` routes it to LANG_FR. Without that the text
    // falls through to LANG_EN and the FR intent_phrases are never
    // consulted.
    let leaks = [
        "Laissez-moi nettoyer les fichiers obsolètes du dépôt.",
        "Laissez-moi supprimer les modèles déjà périmés.",
        "Je vais refactoriser l'intergiciel d'authentification complètement.",
        "Allons réorganiser la structure du dépôt après cette étape.",
        "J'ai besoin de purger les migrations orphelines créées avant.",
        "Je dois ranger l'espace de travail complètement avant la révision.",
        "Besoin de nettoyer les artefacts de build accumulés récemment.",
        "Besoin d'effacer le cache après cette dernière étape complète.",
    ];
    for text in leaks {
        assert!(
            has_forward_intent_post_success(text),
            "FR cleanup-verb intent must fire for: {text:?}"
        );
    }
}

#[test]
fn spanish_cleanup_verb_variants_each_fire() {
    // Each input MUST carry an ES-specific marker (ñ / ¿ / ¡) so
    // `detect_language` routes it to LANG_ES rather than falling
    // through to LANG_FR (FR is the latin-accent fallback once PT
    // and ES markers are absent).
    let leaks = [
        "Déjame limpiar las señales duplicadas en este módulo año tras año.",
        "Déjame eliminar los señalamientos antiguos del año pasado completos.",
        "Voy a refactorizar la pequeña función del año anterior ahora.",
        "Vamos a reorganizar las señales del módulo de configuración antiguo.",
        "Necesito purgar las señales huérfanas del año pasado completas.",
        "Tengo que ordenar el pequeño espacio de trabajo del año.",
        "Debería limpiar las señales obsoletas del año pasado pronto.",
    ];
    for text in leaks {
        assert!(
            has_forward_intent_post_success(text),
            "ES cleanup-verb intent must fire for: {text:?}"
        );
    }
}

#[test]
fn portuguese_cleanup_verb_variants_each_fire() {
    // Each input MUST carry a PT-specific marker (ã / õ / ç) so
    // `detect_language` routes it to LANG_PT rather than falling
    // through to LANG_ES / LANG_FR.
    let leaks = [
        "Deixa eu limpar as importações duplicadas nestes arquivos antigos.",
        "Deixa eu deletar os arquivos antigos da pasta de configuração.",
        "Vou refatorar a função de autenticação completa do módulo.",
        "Vamos reorganizar a estrutura do repositório após esta correção.",
        "Preciso purgar as migrações órfãs pendentes do banco de dados.",
        "Tenho que arrumar a configuração após esta alteração do módulo.",
        "Deveria limpar os artefatos de compilação após a publicação atual.",
    ];
    for text in leaks {
        assert!(
            has_forward_intent_post_success(text),
            "PT cleanup-verb intent must fire for: {text:?}"
        );
    }
}

#[test]
fn russian_cleanup_verb_variants_each_fire() {
    let leaks = [
        "Давайте очистим повторяющиеся импорты в этих файлах сейчас.",
        "Давайте удалим устаревшие тесты из проекта прямо сейчас.",
        "Я отрефакторю промежуточное программное обеспечение аутентификации.",
        "Сейчас реорганизую структуру репозитория полностью и аккуратно.",
        "Мне нужно вычистить устаревшие миграции из базы данных.",
        "Нужно очистить артефакты сборки в каталоге проекта.",
        "Нужно убрать неиспользуемые экспорты из этого модуля.",
    ];
    for text in leaks {
        assert!(
            has_forward_intent_post_success(text),
            "RU cleanup-verb intent must fire for: {text:?}"
        );
    }
}

#[test]
fn cleanup_verbs_dont_fire_on_presentation_phrasing() {
    // Sanity guards across languages. None of these are forward-
    // looking intent — they're presentation / communication phrases
    // that happen to use "clean" / "limpar" / "очистить" etc. as
    // adjectives or completed-state descriptions. The curated
    // intent_phrases lists must NOT match on these.
    let safe = [
        // EN — adjectival "clean", past-tense "cleaned"
        "The repository is now clean and ready for the next release.",
        "I cleaned up the imports before pushing the commit.",
        // FR — past-tense / adjectival
        "Le dépôt est maintenant propre et prêt pour la prochaine version.",
        // ES — past-tense
        "El repositorio quedó limpio y listo para el próximo lanzamiento.",
        // PT — past-tense
        "O repositório ficou limpo e pronto para o próximo lançamento.",
        // RU — adjectival "чист"
        "Репозиторий теперь чист и готов к следующему выпуску.",
    ];
    for text in safe {
        assert!(
            !has_forward_intent_post_success(text),
            "non-intent phrasing must NOT fire the cleanup detector: {text:?}"
        );
    }
}
