### Governance in the Downstream Programs 

This note documents governance related changes/additions we seek in the two 'child' programs

1. Stake Pool Program
2. Lido Program 

It assumes that we have already setup the 'parent' program - i.e. the Governance program (multi-sig or otherwise) and the relevant signatory address for the Governance is at `0xGovernance` 

In the context of the `Serum Multisig` program that we are using, `0xGovernance` refers to PDA discussed in [this comment](https://github.com/ChorusOne/solido/issues/26#issuecomment-829439107)


```From the multisig account and the Multisig program id, we get a program derived address. This is the address that the multisig can sign with, so this is the one that you can set as e.g. the upgrade authority for the Lido program.```


---- 

## 1. Stake Pool Program 


### Upgrade Authority 
- Assuming we are going to deploy our own Stake Pool Program 
  - (and maintain it)
  - We need to set upgrade authority of this deployment to `0xGovernance`
  
### Manager Role
 - Set to `0xGovernance`
  
### Staker Role 

- Set to Lido Program 
    - ~~This might involve a chicken-egg situation at init.~~ [Comment](https://github.com/ChorusOne/solido/pull/55/commits/97815b7586a31bccbfe1936a5eb505a8704df5f1#r633346997)

--- 

## 2. Lido Program

### Upgrade Authority 
- While deploying the Lido program, set upgrade authority to`0xGovernance`
  
### Owner Role 
  - Lido Program has an owner role to be provided at init
  - Set this owner to be`0xGovernance`
  - I am going to reference this as `Lido Owner` in all the notes below. 

### Whitelist for Maintenance Responsibilities 

#### Context 
  - There are certain maintenance actions/instructions in the Lido Program
    - eg. delegating SOL in Lido Program across validators in Stake Pool Program
    - eg. rebalancing stakes 
  - Requiring `Lido Owner` (aka the multisig governance) to sign off each time would be overkill for these responsibilities
  - Opening these to anyone-can-call might be abused by jokers 


#### Solution 

  - Midway Solution - A Whitelist of Maintainers
    - Lido Program to maintain a list of addresses corresponding to maintainers
    - All Maintenance responsibilities are gated to 'onlyMaintainers' 
    - `Lido Owner` is part of the list
    - Additionally, `Lido Owner` can add/remove addresses from this list

  - Potential Implementation
    - Store this list in the Lido State - `Maintainers_List`
    - We will need instructions for `Add Maintainer` / `Remove Maintainer`
      - These two instructions are gated to 'onlyLidoOwner' 

    - Create a method in state.rs : `check_maintainer`
      - This is similar to `check_manager` in the Stake Pool library (`state.rs`)
      - Returns true only if the signer is in the list (and has proper signature)
        - Question for Fynn : How costly will this lookup be? (Say a list of 20 maintainers?)

    - Add `check_maintainer` in the implementation of instructions you want to gate to `onlyMaintainers`

### Gating DelegateDeposits 

  - As discussed above : gate DelegateDeposit to `onlyMaintainers`


### Staker Role Responsibilities
  - `Staker Role` of Stake Pool Program will be the Lido Program 
  - `Staker Role` corresponds to following responsibilities (instructions) in the Stake Pool Program

      ```
    -  1. Set_Staker (needs signature of both manager and staker)
    -  2. CreateValidatorStakeAccount
    -  3. AddValidatorFromPool
    -  4. RemoveValidatorFromPool

    -  5. DecreaseValidatorStake
    -  6. IncreaseValidatorStake

      ```
  - We need to proxy these instructions via the Lido Program 
    - Proxy Instructions 1. to 4. and gate them to `onlyLidoOwner`
    - Proxy Instructions 5., 6. and gate them to `onlyMaintainers` 




### Fee Params 
  - These params are listed in [Fee Management Document](fee-management.md)
  - We need to maintain these params in the Lido Program 
  - We will need to add functions to modify these params - and gate them to onlyLidoOwner


### PauseDeposits
  - Discuss if we need a way to pause deposits
    - We might need this early on, when the Lido Program is initialised but we are still setting up params, validators etc
    - There might be a reason to pause deposits - at some later point too. eg. bug discovered, need to upgrade Lido

  -  Potential Implementation 
     -  A boolean flag `PAUSE_DEPOSITS` initially set to `TRUE`
     -  Only editable by `Lido Owner` (aka 0xGovernance)
     -  Once everything is setup, `Lido Owner` can set this to `FALSE `
     -  The implementation function for Deposit - does an upfront check for `PAUSE_DEPOSITS` - and fails if it is `TRUE`.