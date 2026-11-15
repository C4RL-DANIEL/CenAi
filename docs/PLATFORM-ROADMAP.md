# CenAI platform roadmap

This is the consolidated product plan. Items are not considered shipped until implemented, tested, and documented.

## Confirmed product decisions
- Send.ai: managed control plane.
- Guardian: advanced, resource-conscious diagnostics.
- Voice: hybrid offline and optional online voices.
- Update channels: Stable, Beta, Experimental.
- Rollout: staged.
- Android control: safe, official APIs and scoped user-approved integrations.
- Agent autonomy: approval-based.
- Pricing: hybrid — generous free tier, low-cost Supporter plan, optional API credits.
- Additional modules: Projects, AI Arena, Automations, Code Studio, Knowledge Vault.

## Product systems
1. CenAI Brain and provider-independent Model Hub with task-aware routing.
2. Projects, Knowledge Vault, personal context controls, conversation branching.
3. AI Arena, agent teams, deep research, tutor, meeting and translation assistants.
4. Automations, browser agent, workflow recorder, approval broker, emergency stop.
5. Safe Android Device Agent using official Android APIs and app integrations.
6. Code Studio, App Builder, Creative Studio, GitHub workflows.
7. Hybrid Voice, Live Vision, Offline Brain, downloadable model/voice packs.
8. CenAI API, SDKs, usage analytics, quotas, webhooks, plugin and agent marketplaces.
9. Send.ai control plane for releases, flags, remote config, model/provider registry, Guardian, privacy-conscious analytics, users/RBAC, and audit logs.
10. Guardian diagnostics, post-update health checks, resource-aware reporting, safe feature rollback/disable.
11. Four complete themes: CenAI Default, Material You, iOS-inspired Liquid Glass, CenAI Studio.
12. Language expansion beginning with English and Filipino, followed by validated language/voice coverage.

## Security and quality are cross-cutting requirements
- Protected signing, secrets management, dependency review, SBOM, release provenance.
- Least privilege, tenant isolation, RBAC, MFA for admins, audit logs.
- Tool allowlists, prompt-injection defenses, bounded agent execution, approval for sensitive actions.
- Encryption, data minimization, user export/deletion, privacy controls.
- Unit, integration, UI, API authorization, security, compatibility, offline, performance, update, and recovery tests.
- Stable release gates with documented evidence.

## Delivery phases
1. Audit current Android app, repository, website, backend, Figma, and CI; preserve existing functionality.
2. Security foundation, build pipeline, testing baseline, and design system.
3. Website and Send.ai backend/control plane.
4. Update Center, Guardian, model/provider architecture, and hybrid voice.
5. Projects, Vault, AI Arena, Automations, and Code Studio.
6. API platform, billing, marketplace, browser and device agents.
7. Tutor, research, offline brain, live vision, app builder, and language expansion.
8. Performance, privacy, accessibility, recovery, independent security review, and staged public release.

## Definition of done
A feature is complete only when acceptance criteria, automated tests, integration behavior, security review, documentation, failure handling, and supported-device validation are recorded. A successful build alone is not sufficient.
