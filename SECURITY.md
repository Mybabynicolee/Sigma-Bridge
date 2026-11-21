# Security Policy

## Scope

This policy applies to SigmaBridge source code and releases maintained in this repository.

## Security Considerations

- Treat refresh tokens in `data.txt` and the optional `SIGMA_ACCESS` value as secrets. Do not commit them or include them in screenshots, logs, or support messages.
- The client-facing API listens on `0.0.0.0` over plain HTTP and does not require or validate an API key. Anyone who can reach the listening port may be able to submit requests that use the configured upstream account credentials. Keep access restricted to trusted clients and networks. If remote access is required, place the service behind a properly configured TLS and authentication layer.
- Before sharing configuration, logs, or request examples, remove access tokens, refresh tokens, and other account credentials.
- If a credential is exposed, replace it using the credential provider's supported process and update the local configuration.

## Reporting a Vulnerability

Please do not publish credentials, exploit details, or unpatched vulnerability information in a public issue. If private vulnerability reporting is enabled for this repository, use GitHub's private reporting feature. Otherwise, contact the repository maintainer through a privately published contact channel. No security contact address is specified in this document.

Include the affected component, steps to reproduce, impact, and any relevant version information. Redact secrets from all reports.
