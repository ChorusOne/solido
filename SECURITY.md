# Security

## Reporting a vulnerability

Lido for Solana has a bug bounty program managed by Immunefi at
<https://immunefi.com/bounty/lidoforsolana/>. Bounties are funded by the
[Lido DAO][dao].

**Please report any security-sensitive issues through Immunefi’s secure
platform.**

After you report an issue, you can generally expect a response from Immunefi
within 24 hours. They will triage the issue, and contact the Solido development
team if they determine that the report is valid. If the issue is critical,
Immunefi has the means to reach us quickly.

If a vulnerability requires deploying a new version on-chain, we will prepare
this new version privately, and reach out to the [multisig][multisig] members
to verify and approve the new deployment. After the deployment is complete, we
will publish the source code in this repository.

[dao]:      https://chorusone.github.io/solido/governance
[multisig]: https://chorusone.github.io/solido/administration

## Supported versions

We maintain a single version of Solido. At any time, only the most recent
version tagged with a Git tag is supported. We may release new versions that do
not change the on-chain program. In that case we will not deploy the new version
on-chain.

We only support a single version, because we maintain a single deployment of the
Solido program on mainnet. The `solido` command-line program and maintenance bot
are only intended to be used by the [administration multisig
participants][multisig], who we expect to always use the latest version.

The current deployment is listed at <https://chorusone.github.io/solido/deployments>.
