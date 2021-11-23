# Anker program security

This document outlines our threat model and security considerations for the
Anker program. For the general Solido security policy and how to report
vulnerabilities, see SECURITY.md in the root of the repository.

## Overview

The Anker program is the Solana-side program that implements bSOL (bonded SOL)
support for the [Anchor Protocol][anchorprotocol] on the [Terra][terra]
blockchain. Anchor is a money market between lenders, who deposit UST
stablecoin in return for a stable yield, and borrowers, who borrow that UST by
putting up bonded asses such as bSOL or bETH as collateral.

Bonded assets are nominally pegged to their underlying native token (so 1 bSOL =
1 SOL), but backed by staked assets, in this case Lido’s stSOL. The Anker
program can mint and burn bSOL, and it maintains a reserve of stSOL, such that
for every bSOL minted, the reserve contains 1 SOL worth of stSOL. Because stSOL
is a value-accuring token, over time the value of the reserve will be higher
than what is needed to back the bSOL supply. To restore the peg, we swap the
excess stSOL for UST on an AMM, and the proceeds go to the Anchor protocol on
Terra (through [Wormhole][wormhole]), which uses them to provide the yield for
lenders.

To avoid confusion with [the Solana development framework / eDSL that is also
called Anchor][serum-anchor], we named our program “Anker”.

[anchorprotocol]: https://anchorprotocol.com/
[terra]:          https://www.terra.money/
[wormhole]:       https://wormholebridge.com/
[serum-anchor]:   https://github.com/project-serum/anchor

## Components

As a cross-chain protocol, bSOL support for the Anchor protocol involves
multiple components:

 * The Anker program on the Solana blockchain. This program accepts stSOL
   deposits and mints bSOL in return, and when users return their bSOL, the
   program burns it and returns stSOL. This means that bSOL is natively a Solana
   SPL token, and the bSOL on Terra will be bridged. The source code of this
   program is in the `anker` directory of <https://github.com/ChorusOne/solido>.

 * The bSOL contract on the Terra blockchain. For technical reasons, on the
   Terra side the Wormhole-wrapped bSOL tokens need to be wrapped once more, and
   this contract is responsible for that. The repository is not public while
   the contract is being developed, but it will be analogous to the
   existing bETH contract at
   <https://github.com/Anchor-Protocol/anchor-bEth-contracts>.

 * The Anchor contract itself on the Terra blockchain.

 * Wormhole bridge, which is used in a few places:
   * To get UST to Solana in the first place. (It lives natively on Terra.)
   * To send the bSOL that users receive to Terra, so they can deposit it into
     Anchor. This step will have to be done manually by users.
   * To send the UST proceeds of the staking rewards to Terra.

 * Solido, the Lido for Solana program. The Anker program holds stSOL, which is
   minted by Solido, and aside from that, Anker inspects the Solido state to
   obtain the stSOL/SOL exchange rate, in order to compute bSOL/stSOL exchange
   rate that is needed to maintain the 1 bSOL = 1 SOL peg.

 * An AMM for swapping stSOL for UST. We intend to use [Orca][orca] for this,
   which is a deployment of the [SPL Token Swap][spl-token-swap] program.

[orca]:           https://www.orca.so/
[spl-token-swap]: https://github.com/solana-labs/solana-program-library/tree/master/token-swap

This repository only contains the Anker program (and the Solido program), and
only the Anker program is in scope for the purpose of auditing (and in the
future, for the bug bounty program). The bSOL contracts on the Terra side will
be audited separately.

## Roles

There are two types of actors involved in the Anker program:

 * **The manager**. The manager is the upgrade authority of the Anker program,
   and it can sign configuration changes, such as the destination address for
   UST rewards. The manager will be the same multisig instance that also acts as
   the manager for the Solido program. We trust the manager and assume that the
   manager acts in the best interest of the Anker program. We assume that the
   maintainer will configure the Anker program correctly.

 * **Users**. Users deposit stSOL and receive bSOL in return.

There is no separate “maintainer” role like in the Solido program. The Anker
program is intended to be fully permissionless, and aside from configuration
changes, there are no privileged instructions. In particular, swapping stSOL
for UST is something that can be done by anybody.

## Functionality

The Anker program has three main functions for everyday use:

 * **Deposit**, where users deposit stSOL and receive bSOL in return.
 * **Withdraw**, where users redeem their stSOL by returning the bSOL.
 * **Claim Rewards**, where, if the value of the stSOL in the reserve is greater
   than what is needed to back the bSOL supply at a 1 bSOL = 1 SOL exchange
   rate, the program can swap the excess stSOL for UST against an AMM, and it
   sends the proceeds through Wormhole to a preconfigured address on Terra.

None of these functions are privileged. In practice the existing Solido
maintainer bot is going to be responsible for calling *Claim Rewards* when
possible. Note that, because the Solido exchange rate changes at most once per
epoch, claiming rewards is possible at most once per epoch.

## Singleton Anker instance

We follow Neodyme’s recommendation about making the Anker instance a singleton,
with one modification: because Solido is not (enforced to be) a singleton, we
have one Anker instance per Solido instance. The Anker instance lives at a
program-derived address that is derived from the Solido instance’s address. This
ensures that there is one unique Anker instance associated with every Solido
instance.

## Scope and trust

For auditing, and a future bug bounty, the scope is limited to the Anker program
and its responsibilities. Examples of issues we would like to know about:

 * Minting bSOL without providing the backing stSOL of the right value.
 * Withdrawing stSOL without returning any bSOL of the right value.
 * Preventing future deposits or withdrawals.
 * Changing configuration without signature from the manager.
 * Sending UST proceeds to a different recipient than the address configured in
   the Anker instance.

Once we mint the bSOL, what happens to it (either on Solana or on Terra) is no
longer the concern of the Anker program. Examples of issues out of scope:

 * Vulnerabilities in the bSOL contract on the Terra side (it will be audited
   separately).
 * Attacks that involve manipulating Wormhole.

The Anker program necessarily interacts with other programs, and those programs
are upgradeable. Therefore:

 * We trust the Orca Swap program and its upgrade authority.
 * We trust the Wormhole program and its authority.

We are interested in minimizing the impact that a compromised Orca or Wormhole
program could have, but in principle we trust these programs.

## Further resources

 * [Anchor Protocol documentation](https://docs.anchorprotocol.com/)
 * [Lido for Solana documentation](https://docs.solana.lido.fi/)
 * [Anchor bAssets guide](https://docs.google.com/document/d/1tvw_hHBRhLSLNCNOWxjfQU86jtQmR-KSfUoh-mhEWUo/edit)
