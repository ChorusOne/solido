#!/usr/bin/env python3

"""
This script has multiple options to to interact with Solido
"""
from typing import Any, Dict

Sample: Dict[str, Any] = {
    'solido_instance': '2i2crMWRb9nUY6HpDDp3R1XAXXB9UNdWAdtD9Kp9eUNT',
    'manager': '2cAVMSn3bfTyPBMnYYY3UgqvE44SM2LLqT7iN6CiMF4T',
    'validator_vote_account': '2FaFw4Yv5noJfa23wKrFDpqoxXo8MxQGbKP3LjxMiJ13',
    'program_to_upgrade': '2QYdJZhBrg5wAvkVA98WM2JtTXngsTpBXSq2LXnVUa33',
    'program_data_address': 'HZe59cxGy7irFUtcmcUwkmvERrwyCUKaJQavwt7VrVTg',
    'buffer_address': '2LCfqfcQBjKEpvyA54vwAGaYTUXt1L13MwEsDbrzuJbw',
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
ValidatorSetV2 = set()
SolidoVersion = -1
SolidoState = "Unknown state"


def printSolution(flag: bool) -> str:
    if flag :
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
    return printSolution(SolidoState == state)

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
        retbuf += printSolution(value == Sample.get('reward_distribution').get(key))
    else:
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

def verify_solido_state(json_data: Any) -> None:
    # parse solido state
    l1_keys = json_data.get('solido')
    global SolidoVersion
    SolidoVersion = l1_keys.get('lido_version')
    for validator in l1_keys.get('validators').get('entries') :
        vote_acc = validator.get('pubkey')
        if validator.get('entry').get('active') == True :
            ValidatorSetV1.add(vote_acc)

    # detect current state
    global SolidoState
    if SolidoVersion == 0:
        if len(ValidatorSetV1) == 21:
            SolidoState = "Deactivate validators"
        elif len(ValidatorSetV1) == 0:
            SolidoState = "Upgrade program"
    elif SolidoVersion == 1 and len(ValidatorSetV1) == 0:
        SolidoState = "Add validators"
    else:
        SolidoState = "Unknown state: solido version = " 
        + str(SolidoVersion) + " active validators count = "
        + str(len(ValidatorSetV1))
    
    #output result
    print("\nCurrent migration state: " + SolidoState + "\n")

def verify_transaction_data(json_data: Any) -> bool:
    # print(json_data)
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
            output_buf += ValidateDeactivateV1VoteAccount(trans_data, 'validator_vote_account')
        elif 'AddValidator' in l2_data.keys():
            output_buf += "AddValidator"
            output_buf += ValidateSolidoState("Add validators")
            trans_data = l2_data['AddValidator']
            output_buf += ValidateField(trans_data, 'solido_instance')
            output_buf += ValidateField(trans_data, 'manager')
            output_buf += ValidateAddV2VoteAccount(trans_data, 'validator_vote_account')
        elif 'MigrateStateToV2' in l2_data.keys():
            output_buf += "MigrateStateToV2"
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
            output_buf += ValidateRewardField(reward_distribution, 'st_sol_appreciation')
        else:
            output_buf += "Unknown instruction\n"
            VerificationStatus = False
    elif 'BpfLoaderUpgrade' in l1_keys.keys():
        output_buf += "BpfLoaderUpgrade"
        output_buf += ValidateSolidoState("Upgrade program")
        l2_data = l1_keys['BpfLoaderUpgrade']
        output_buf += ValidateField(l2_data, 'program_to_upgrade')
        output_buf += ValidateField(l2_data, 'program_data_address')
        output_buf += ValidateField(l2_data, 'buffer_address')
    else:
        output_buf += "Unknown instruction\n"
        VerificationStatus = False

    output_buf += "\n"
    print(output_buf)
    return VerificationStatus
