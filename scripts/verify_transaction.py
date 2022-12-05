#!/usr/bin/env python3

"""
This script has multiple options to to interact with Solido
"""

import sys
import os.path
from typing import Any, Dict, Set, List

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
sys.path.append(os.path.dirname(SCRIPT_DIR))

from tests.util import solido, solana, run  # type: ignore

Sample: Dict[str, Any] = {
    'solido_instance': '2i2crMWRb9nUY6HpDDp3R1XAXXB9UNdWAdtD9Kp9eUNT',  # "solido_address": "49Yi1TKkNyYjPAFdR9LBvoHcUjuPX4Df5T5yv39w2XTn",
    'program_to_upgrade': '2QYdJZhBrg5wAvkVA98WM2JtTXngsTpBXSq2LXnVUa33',  # solido_config.json : solido_program_id
    'program_data_address': 'HZe59cxGy7irFUtcmcUwkmvERrwyCUKaJQavwt7VrVTg',
    'buffer_address': '2LCfqfcQBjKEpvyA54vwAGaYTUXt1L13MwEsDbrzuJbw',  # buffer adres account
    'validator_list': 'HDLRixNLF3PLBMfxhKgKxeZEDhA84RiRUSZFm2zwimeE',
    'maintainer_list': '2uLFh1Ec8NP1fftKD2MLnF12Kw4CTXNHhDtqsWVz7f9K',
    'developer_account': '5vgbVafXQiVb9ftDix1NadV7D6pgP5H9YPCaoKcPrBxZ',
    'reward_distribution': {
        'treasury_fee': 4,
        'developer_fee': 1,
        'st_sol_appreciation': 95,
    },
    'max_validators': 6700,
    'max_maintainers': 5000,
    'max_commission_percentage': 5,
}


ValidatorVoteAccSet = set()
VerificationStatus = True
ValidatorSetV1 = set()
ValidatorSetV2: Set[str] = set()  # will be filled later
SolidoVersion = -1
SolidoState = "Unknown state"
TransOrder: List[str] = list()


def printSolution(flag: bool) -> str:
    if flag:
        return " [OK]\n"
    else:
        global VerificationStatus
        VerificationStatus = False
        return " [BAD]\n"


def checkSolidoState(state: str) -> bool:
    return SolidoState == state


def checkVoteInV1Set(address: str) -> bool:
    return address in ValidatorSetV1


def checkVoteInV2Set(address: str) -> bool:
    return address in ValidatorSetV2


def checkVoteUnic(address: str) -> bool:
    if address not in ValidatorVoteAccSet:
        ValidatorVoteAccSet.add(address)
        return True
    else:
        return False


def ValidateSolidoState(state: str) -> str:
    return ": Solido state " + state + printSolution(SolidoState == state)


def ValidateField(dataDict: Any, key: str) -> str:
    value = dataDict.get(key)
    retbuf = key + " " + str(value)
    if key in dataDict.keys():
        retbuf += printSolution(value == Sample.get(key))
    else:
        retbuf += printSolution(False)
    return retbuf


def ValidateRewardField(dataDict: Any, key: str) -> str:
    value = dataDict.get(key)
    retbuf = key + " " + str(value)
    if key in dataDict.keys():
        reward_distribution = Sample.get('reward_distribution')
        if reward_distribution is not None:
            sampleValue = reward_distribution.get(key)
            if sampleValue != None:
                retbuf += printSolution(value == sampleValue)
                return retbuf

    retbuf += printSolution(False)
    return retbuf


def ValidateDeactivateV1VoteAccount(dataDict: Any, key: str) -> str:
    value = dataDict.get(key)
    retbuf = key + " " + str(value)
    if key in dataDict.keys():
        retbuf += printSolution(checkVoteUnic(value) and checkVoteInV1Set(value))
    else:
        retbuf += printSolution(False)
    return retbuf


def ValidateAddV2VoteAccount(dataDict: Any, key: str) -> str:
    value = dataDict.get(key)
    retbuf = key + " " + str(value)
    if key in dataDict.keys():
        retbuf += printSolution(checkVoteUnic(value) and checkVoteInV2Set(value))
    else:
        retbuf += printSolution(False)
    return retbuf


def ValidateTransOrder(trans):
    retbuf = "Transaction order "
    if trans == "BpfLoaderUpgrade":
        retbuf += "BpfLoaderUpgrade"
        retbuf += printSolution(len(TransOrder) == 0)
    elif trans == "MigrateStateToV2":
        retbuf += "MigrateStateToV2"
        retbuf += printSolution(
            len(TransOrder) == 1 and TransOrder[0] == "BpfLoaderUpgrade"
        )
    else:
        retbuf += printSolution(False)
    return retbuf


def verify_solido_state() -> None:
    # get solido state
    json_data = solido('--config', os.getenv("SOLIDO_CONFIG"), 'show-solido')

    # parse solido state
    l1_keys = json_data.get('solido')
    global SolidoVersion
    SolidoVersion = l1_keys.get('lido_version')
    validators = l1_keys.get('validators')
    if validators != None:
        for validator in validators.get('entries'):
            vote_acc = validator.get('pubkey')
            if validator.get('entry').get('active') == True:
                ValidatorSetV1.add(vote_acc)

    # detect current state
    global SolidoState
    if SolidoVersion == 0:
        if len(ValidatorSetV1) == 21:
            SolidoState = "Deactivate validators"
        elif len(ValidatorSetV1) == 0:
            SolidoState = "Upgrade program"
        else:
            SolidoState = "Unknown state - solido version = "
            SolidoState += str(SolidoVersion)
            SolidoState += " active validators count = "
            SolidoState += str(len(ValidatorSetV1))
    elif SolidoVersion == 1 and len(ValidatorSetV1) == 0:
        SolidoState = "Add validators"
    else:
        SolidoState = "Unknown state - solido version = "
        SolidoState += str(SolidoVersion)
        SolidoState += " active validators count = "
        SolidoState += str(len(ValidatorSetV1))

    # output result
    print("\nCurrent migration state: " + SolidoState)


def verify_transaction_data(json_data: Any) -> bool:
    l1_keys = json_data['parsed_instruction']
    output_buf = ""
    global VerificationStatus
    VerificationStatus = True
    if 'SolidoInstruction' in l1_keys.keys():
        output_buf += "SolidoInstruction "
        l2_data = l1_keys['SolidoInstruction']
        if 'DeactivateValidator' in l2_data.keys():
            output_buf += "DeactivateValidator"
            output_buf += ValidateSolidoState("Deactivate validators")
            trans_data = l2_data['DeactivateValidator']
            output_buf += ValidateField(trans_data, 'solido_instance')
            output_buf += ValidateField(trans_data, 'manager')
            output_buf += ValidateDeactivateV1VoteAccount(
                trans_data, 'validator_vote_account'
            )
        elif 'AddValidator' in l2_data.keys():
            output_buf += "AddValidator"
            output_buf += ValidateSolidoState("Add validators")
            trans_data = l2_data['AddValidator']
            output_buf += ValidateField(trans_data, 'solido_instance')
            output_buf += ValidateField(trans_data, 'manager')
            output_buf += ValidateAddV2VoteAccount(trans_data, 'validator_vote_account')
        elif 'MigrateStateToV2' in l2_data.keys():
            output_buf += ValidateTransOrder("MigrateStateToV2")
            output_buf += ValidateSolidoState("Upgrade program")
            trans_data = l2_data.get('MigrateStateToV2')
            output_buf += ValidateField(trans_data, 'solido_instance')
            output_buf += ValidateField(trans_data, 'manager')
            output_buf += ValidateField(trans_data, 'validator_list')
            output_buf += ValidateField(trans_data, 'maintainer_list')
            output_buf += ValidateField(trans_data, 'developer_account')
            output_buf += ValidateField(trans_data, 'max_maintainers')
            output_buf += ValidateField(trans_data, 'max_validators')
            output_buf += ValidateField(trans_data, 'max_commission_percentage')

            reward_distribution = trans_data.get('reward_distribution')
            output_buf += ValidateRewardField(reward_distribution, 'treasury_fee')
            output_buf += ValidateRewardField(reward_distribution, 'developer_fee')
            output_buf += ValidateRewardField(
                reward_distribution, 'st_sol_appreciation'
            )
        else:
            output_buf += "Unknown instruction\n"
            VerificationStatus = False
    elif 'BpfLoaderUpgrade' in l1_keys.keys():
        output_buf += ValidateTransOrder("BpfLoaderUpgrade")
        TransOrder.append("BpfLoaderUpgrade")
        output_buf += ValidateSolidoState("Upgrade program")
        l2_data = l1_keys['BpfLoaderUpgrade']
        output_buf += ValidateField(l2_data, 'program_to_upgrade')
        output_buf += ValidateField(l2_data, 'program_data_address')
        output_buf += ValidateField(l2_data, 'buffer_address')
    else:
        output_buf += "Unknown instruction\n"
        VerificationStatus = False

    print(output_buf)
    return VerificationStatus


def verify_transactions(ifile):
    Counter = 0
    Success = 0
    for transaction in ifile:
        result = solido(
            '--config',
            os.getenv("SOLIDO_CONFIG"),
            'multisig',
            'show-transaction',
            '--transaction-address',
            transaction.strip(),
        )
        Counter += 1
        print("Transaction #" + str(Counter) + ": " + transaction.strip())
        if verify_transaction_data(result):
            Success += 1
    print(
        "Summary: successfully verified "
        + str(Success)
        + " from "
        + str(Counter)
        + " transactions"
    )


if __name__ == '__main__':
    print("main")
