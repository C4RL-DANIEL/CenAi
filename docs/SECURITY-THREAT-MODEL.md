# CenAI security threat model

Status: implementation requirements; not a security certification.

## Assets
- Release signing key and build credentials
- Provider credentials and OAuth refresh tokens
- User accounts, sessions, conversations, projects, files, and memory
- API keys, quota balances, billing records, and audit events
- Release artifacts, update metadata, and feature configuration

## Trust boundaries
1. Android client ↔ CenAI API
2. CenAI API ↔ provider APIs
3. Send.ai admin console ↔ privileged control-plane API
4. CI runner ↔ signing and release infrastructure
5. AI model output ↔ tool execution broker
6. User device ↔ external Android applications

## Primary threats and required mitigations
- APK repackaging/reverse engineering: signing verification, integrity signals, server-side entitlement enforcement; assume client code is inspectable.
- Secret exposure: no master secrets in APK, repository, logs, or public artifacts; use secret manager and rotation.
- Account/session takeover: short-lived access tokens, refresh rotation, revocation, secure storage, MFA for administrators.
- IDOR/cross-tenant access: server-side object ownership checks on every request and automated negative authorization tests.
- Prompt injection/tool abuse: treat model output and documents as untrusted; tool allowlists, schema validation, scoped capabilities, approval gates, bounded execution.
- Supply-chain compromise: pinned actions/dependencies where feasible, dependency scanning, SBOM, protected branches, review, provenance.
- Malicious update: signed artifact, SHA-256 checksum, signer continuity, channel/compatibility validation, staged rollout.
- Admin compromise: RBAC, MFA, least privilege, step-up confirmation, immutable audit trail, emergency revocation.
- Data leakage: encryption in transit/at rest, project isolation, data minimization, retention limits, export/deletion workflows.

## Mandatory release gates
No Stable release with an unresolved critical security defect, failed core workflow, broken update integrity, or uncertain data integrity. High-severity findings require documented risk acceptance and remediation plan before release.

## Operational caveats
Android ordinary apps cannot silently install arbitrary APKs or fully control other apps. Device Agent capabilities must use supported Android APIs and explicit permissions. Play Integrity is a signal, not a substitute for authorization. Obfuscation is not source-code secrecy.
