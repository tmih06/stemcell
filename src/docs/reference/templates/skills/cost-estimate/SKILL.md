---
name: cost-estimate
description: Scan a codebase and produce a professional cost-to-build estimate, AI-assisted ROI breakdown, and fair-market valuation. Counts files/LOC, rates complexity, estimates human team hours across 4 team sizes, computes annual maintenance burden, and asks the user for business context before producing a blended valuation range. Triggers on "/cost-estimate", "what's this codebase worth", "valuation", "cost to rebuild", "value per AI hour", "is this project worth", "estimate the build cost".
---

Scan this entire codebase and produce a professional cost estimate and valuation report. Analyze:

1. **Codebase inventory**: Count files, lines of code by language, modules, API integrations, external services, database schemas, UI components, and any complex subsystems.

2. **Complexity assessment**: Identify the hardest parts — real-time features, protocol implementations, security layers, multi-platform support, API integrations (especially government/enterprise APIs that require domain expertise), custom parsers, streaming, WebSocket/SSE, OAuth flows, etc.

3. **Human team estimate**: Calculate what a real development team would need to build this from scratch. Use current US market rates (2025-2026):
   - Senior full-stack developer: $125-175/hr
   - Backend specialist: $150-200/hr
   - DevOps/infra: $140-180/hr
   - UI/UX: $100-150/hr
   - Project management overhead: 15-20%
   - QA/testing: 15-20% of dev time
   - Estimate across 4 team sizes: Solo dev, Lean Startup (2-3), Growth Co (4-6), Enterprise (8+)

4. **AI comparison**: Estimate AI-assisted hours actually spent (based on git history, commit frequency, time span from first to latest commit). Calculate speed multiplier and value per hour.

5. **Integration complexity**: For each external integration (APIs, channels, protocols, third-party services), assess:
   - API stability and breaking change risk (how often does the upstream API change?)
   - Authentication complexity (OAuth, tokens, QR pairing, binary handshakes)
   - Rate limiting and quota constraints
   - Failure modes and required retry/fallback logic
   - Vendor lock-in risk and migration difficulty
   - Rate each integration: Low / Medium / High / Critical maintenance burden

6. **Test coverage and CI**: Analyze what exists and what a production build would need:
   - Current test coverage (count ALL test types: `#[test]`, `#[tokio::test]`, `#[rstest]`, proptest — not just `#[test]`)
   - Missing coverage gaps (what subsystems have zero tests?)
   - Estimated hours to reach production-grade coverage (70-80%)
   - CI pipeline requirements (build matrix, linting, security scanning, release automation)
   - Cost of CI infrastructure (GitHub Actions minutes, build times for Rust)

7. **Ongoing maintenance and operational cost**: The hidden costs after "it works":
   - Monthly maintenance hours by category (dependency updates, security patches, API breaking changes, bug fixes)
   - On-call burden estimate — how many integration points can break independently? What's the expected incident frequency?
   - Dependency risk — count direct deps, assess which are unmaintained/fragile/pre-1.0
   - Upgrade burden — major version bumps expected in next 12 months
   - Annual maintenance cost (hours x rate) for a solo maintainer vs. a team
   - Technical debt estimate — what shortcuts exist that will cost more later?

8. **Fair market valuation**: Before estimating valuation, **ASK THE USER** for context that affects the valuation model. Prompt them with:
   > "To produce an accurate valuation, I need some context:
   > 1. **Business model** — Is this OSS, SaaS, enterprise-licensed, consulting, or something else?
   > 2. **Revenue** — Any current MRR/ARR? If pre-revenue, is monetization planned?
   > 3. **Traction** — GitHub stars, clones, downloads, active users, community size?
   > 4. **Team** — Solo maintainer or team? Full-time or side project?
   > 5. **Funding** — Bootstrapped, funded, or seeking investment?
   > 6. **Intent** — Are you valuing for acquisition, fundraising, insurance, or just curiosity?"

   Wait for the user's answers, then use the appropriate valuation methods:

   **Always include:**
   - **Cost-to-reproduce** — what would it cost to rebuild from scratch today? Use the Grand Total figures.
   - **Replacement cost** — what would a company pay to buy equivalent functionality off the shelf? If no equivalent exists, note that — it increases strategic value.
   - **Strategic/acqui-hire value** — what would an acquirer pay for the technology + expertise? Consider: unique integrations, competitive moat, time-to-market advantage, and talent cost savings.
   - **Risk-adjusted valuation** — discount for: bus factor, technical debt, test coverage gaps, dependency risks, market competition.

   **Include if applicable (based on user answers):**
   - **Revenue multiple** — only if there's actual or planned revenue. Apply industry multiples (3-8x dev tools, 5-15x AI/infrastructure).
   - **OSS traction valuation** — if open source: use $/star benchmarks from historical acquisitions, community growth rate, clone/download metrics, projected trajectory.
   - **Funding-stage valuation** — if seeking investment: comparable seed/Series A rounds for similar dev tools.

   - **Valuation summary table** — show Low / Mid / High estimates across all applicable methods, then a blended fair market range.

9. **Output a report** with these sections:
   - Codebase Overview (languages, LOC, modules, integrations)
   - Complexity Breakdown (table of subsystems with difficulty rating and estimated hours)
   - Integration Risk Matrix (table: integration, auth type, API stability, breaking change risk, maintenance burden)
   - Test Coverage Analysis (current state, gaps, cost to reach production grade)
   - CI/CD Requirements (what's needed, estimated setup hours, monthly cost)
   - Value per AI-Assisted Hour (table)
   - Speed vs. Human Developer comparison
   - Cost Comparison (human cost vs AI-assisted cost with net savings and ROI)
   - Grand Total Summary (table across all 4 team sizes with calendar time, human hours, total cost)
   - Ongoing Maintenance (annual cost table: solo vs. team, broken down by category)
   - On-Call Burden (expected incidents/month, integration failure points, blast radius)
   - Fair Market Valuation (all methods, risk adjustments, blended range)
   - The Headline (one italic paragraph summarizing the key insight)
   - Assumptions (numbered list of caveats)

Be thorough but honest. Base estimates on real market rates and realistic timelines. Don't inflate numbers — credibility matters more than impressive figures. The goal is to show the build cost, true cost of ownership, AND what the project is actually worth.
