//! Shared allowlist / respond-to policy for inbound messages.
//!
//! Every channel today re-implements the same decision: given the sender, the
//! conversation, and whether it's a DM/mention, should the bot respond? The
//! policy inputs (`allowed_users`, `allowed_channels`, `respond_to`) live on
//! each per-channel config struct with identical shapes. This module reads the
//! right struct by `surface_id` and applies the one shared policy.
//!
//! Platform-specific facts (is this a DM? was the bot mentioned?) are computed
//! by the surface and carried on [`Inbound::routing`]; this service never
//! inspects platform APIs.

use crate::channels::gateway::envelope::Inbound;
use crate::config::{Config, RespondTo};

/// Outcome of the allowlist check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllowlistDecision {
    /// Respond to this message.
    Respond,
    /// Drop it; `reason` is for debug logging only.
    Ignore { reason: String },
}

/// Normalized view of one surface's gating config. Built per `surface_id` so
/// the policy below is written once.
struct Policy<'a> {
    allowed_users: &'a [String],
    allowed_channels: &'a [String],
    respond_to: &'a RespondTo,
}

/// Resolve the gating policy for a surface from config, or `None` for surfaces
/// that have no allowlist (the TUI is always-respond; unknown ids too).
fn policy_for<'a>(surface_id: &str, cfg: &'a Config) -> Option<Policy<'a>> {
    let c = &cfg.channels;
    match surface_id {
        "telegram" => Some(Policy {
            allowed_users: &c.telegram.allowed_users,
            allowed_channels: &c.telegram.allowed_channels,
            respond_to: &c.telegram.respond_to,
        }),
        "discord" => Some(Policy {
            allowed_users: &c.discord.allowed_users,
            allowed_channels: &c.discord.allowed_channels,
            respond_to: &c.discord.respond_to,
        }),
        "slack" => Some(Policy {
            allowed_users: &c.slack.allowed_users,
            allowed_channels: &c.slack.allowed_channels,
            respond_to: &c.slack.respond_to,
        }),
        // WhatsApp gates on phone numbers via `allowed_phones`; Trello has its
        // own per-board access model. Those surfaces pass their own gate before
        // publishing, so here they are always-respond. The TUI likewise has no
        // allowlist.
        _ => None,
    }
}

/// Apply the shared allowlist + respond-to policy to an inbound message.
pub fn evaluate(inbound: &Inbound, cfg: &Config) -> AllowlistDecision {
    let Some(policy) = policy_for(inbound.surface_id, cfg) else {
        // No gating configured for this surface — always respond.
        return AllowlistDecision::Respond;
    };

    // User allowlist (empty = everyone allowed). Applies in DMs and groups.
    if !policy.allowed_users.is_empty()
        && !policy.allowed_users.iter().any(|u| u == &inbound.sender.id)
    {
        return AllowlistDecision::Ignore {
            reason: format!("sender {} not in allowed_users", inbound.sender.id),
        };
    }

    // DMs always pass the channel + respond_to filters.
    if inbound.routing.is_direct {
        return AllowlistDecision::Respond;
    }

    // Channel allowlist (empty = all channels allowed).
    if !policy.allowed_channels.is_empty()
        && !policy
            .allowed_channels
            .iter()
            .any(|ch| ch == &inbound.conversation_key)
    {
        return AllowlistDecision::Ignore {
            reason: format!(
                "conversation {} not in allowed_channels",
                inbound.conversation_key
            ),
        };
    }

    match policy.respond_to {
        RespondTo::All => AllowlistDecision::Respond,
        RespondTo::DmOnly => AllowlistDecision::Ignore {
            reason: "respond_to=dm_only and this is a group message".to_string(),
        },
        RespondTo::Mention => {
            if inbound.routing.is_mention {
                AllowlistDecision::Respond
            } else {
                AllowlistDecision::Ignore {
                    reason: "respond_to=mention and bot was not mentioned".to_string(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::gateway::envelope::{Routing, SenderRef};

    fn cfg_with_discord(
        allowed_users: Vec<String>,
        allowed_channels: Vec<String>,
        respond_to: RespondTo,
    ) -> Config {
        let mut cfg = Config::default();
        cfg.channels.discord.allowed_users = allowed_users;
        cfg.channels.discord.allowed_channels = allowed_channels;
        cfg.channels.discord.respond_to = respond_to;
        cfg
    }

    fn discord_msg(sender_id: &str, conv: &str, routing: Routing) -> Inbound {
        let mut inb = Inbound::new("discord", conv, SenderRef::new(sender_id, "X"), "hi");
        inb.routing = routing;
        inb
    }

    #[test]
    fn unknown_surface_always_responds() {
        let cfg = Config::default();
        let inb = Inbound::new("tui", "sess-1", SenderRef::new("u", "U"), "hi");
        assert_eq!(evaluate(&inb, &cfg), AllowlistDecision::Respond);
    }

    #[test]
    fn empty_user_allowlist_accepts_anyone() {
        let cfg = cfg_with_discord(vec![], vec![], RespondTo::All);
        let inb = discord_msg("999", "chan", Routing::default());
        assert_eq!(evaluate(&inb, &cfg), AllowlistDecision::Respond);
    }

    #[test]
    fn non_allowed_user_is_ignored() {
        let cfg = cfg_with_discord(vec!["111".into()], vec![], RespondTo::All);
        let inb = discord_msg("999", "chan", Routing::default());
        assert!(matches!(
            evaluate(&inb, &cfg),
            AllowlistDecision::Ignore { .. }
        ));
    }

    #[test]
    fn dm_bypasses_respond_to_and_channel_filters() {
        // respond_to=mention, but a DM (is_direct) with no mention still passes.
        let cfg = cfg_with_discord(vec![], vec!["other-chan".into()], RespondTo::Mention);
        let inb = discord_msg(
            "999",
            "dm-chan",
            Routing {
                is_direct: true,
                is_mention: false,
            },
        );
        assert_eq!(evaluate(&inb, &cfg), AllowlistDecision::Respond);
    }

    #[test]
    fn group_message_in_non_allowed_channel_is_ignored() {
        let cfg = cfg_with_discord(vec![], vec!["allowed-chan".into()], RespondTo::All);
        let inb = discord_msg(
            "999",
            "other-chan",
            Routing {
                is_direct: false,
                is_mention: false,
            },
        );
        assert!(matches!(
            evaluate(&inb, &cfg),
            AllowlistDecision::Ignore { .. }
        ));
    }

    #[test]
    fn mention_policy_requires_mention_in_group() {
        let cfg = cfg_with_discord(vec![], vec![], RespondTo::Mention);
        let not_mentioned = discord_msg(
            "999",
            "chan",
            Routing {
                is_direct: false,
                is_mention: false,
            },
        );
        assert!(matches!(
            evaluate(&not_mentioned, &cfg),
            AllowlistDecision::Ignore { .. }
        ));

        let mentioned = discord_msg(
            "999",
            "chan",
            Routing {
                is_direct: false,
                is_mention: true,
            },
        );
        assert_eq!(evaluate(&mentioned, &cfg), AllowlistDecision::Respond);
    }

    #[test]
    fn dm_only_policy_ignores_group_messages() {
        let cfg = cfg_with_discord(vec![], vec![], RespondTo::DmOnly);
        let inb = discord_msg(
            "999",
            "chan",
            Routing {
                is_direct: false,
                is_mention: true,
            },
        );
        assert!(matches!(
            evaluate(&inb, &cfg),
            AllowlistDecision::Ignore { .. }
        ));
    }
}
