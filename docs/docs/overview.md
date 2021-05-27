---
id: overview
title: overview
description: Overview of LIDO for Solana
keywords:
 - lido
 - solido
 - solana
slug: /
sidebar_position: 1
---

# Overview

## Lido for Solana

'Lido for Solana' is a Lido-DAO governed liquid staking protocol for the Solana blockchain. Anyone who stakes their SOL tokens with Lido will be issued an on-chain representation of SOL staking position with Lido validators, called **stSOL**. We will work to integrate stSOL widely into the Solana DeFi ecosystem to enable stSOL users to make use of their staked assets in a variety of applications.

Lido for Solana gives you:
- **Liquidity** — No delegation/activation delays and the ability to sell your staked tokens
- **One-click staking** — No complicated steps
- **Decentralized security** — Assets spread across the industry’s leading validators chosen by the Lido DAO

## How Lido for Solana works
Lido for Solana not only makes it very easy to stake but also provides further utility through stSOL. Let’s look at the process in slight detail. A SOL token holder connects their wallet to an interface that supports Lido (one will e.g. be hosted at https://stake.lido.fi) and deposits their tokens into the Lido program. They immediately receive stSOL tokens that represent a share of the total pool. Every user’s tokens are first held in a pool controlled by the Lido program

![How Solido works](./howsolidoworks.png)

The Lido program collects the deposited SOL and releases the newly minted stSOL to the user. Beneath the layer, the Lido Program leverages the Stake Pool Program Library to distribute this SOL uniformly across validators participating in the Lido Program. When these delegations accrue rewards on the allotted stake, the total SOL under stake pool management increases and this increases the value of stSOL tokens. The Lido DAO governs the Lido Program and the underlying Stake Pool program — and also controls the list of validators that are part of this program.

