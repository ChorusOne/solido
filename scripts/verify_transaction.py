#!/usr/bin/env python3

"""
This script has multiple options to to interact with Solido
"""


Sample = {
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


def CheckField(dataDict, key):
    if key in dataDict.keys():
        retbuf = key + " " + str(dataDict.get(key))
        if dataDict.get(key) == Sample.get(key):
            retbuf += " [OK]\n"
        else:
            retbuf += " [BAD]\n"
            VerificationStatus = False
        return retbuf


def CheckRewardField(dataDict, key):
    if key in dataDict.keys():
        retbuf = key + " " + str(dataDict.get(key))
        if dataDict.get(key) == Sample.get('reward_distribution').get(key):
            retbuf += " [OK]\n"
        else:
            retbuf += " [BAD]\n"
            VerificationStatus = False
        return retbuf


def CheckVoteAccField(dataDict, key):
    if key in dataDict.keys():
        retbuf = key + " " + dataDict.get(key)
        if dataDict.get(key) not in ValidatorVoteAccSet:
            ValidatorVoteAccSet.add(dataDict.get(key))
            retbuf += " [OK]\n"
        else:
            retbuf += " [BAD]\n"
            VerificationStatus = False
        return retbuf


def verify_transaction_data(json_data):
    # print(json_data)
    l1_keys = json_data['parsed_instruction']
    output_buf = ""
    VerificationStatus = True
    if 'SolidoInstruction' in l1_keys.keys():
        output_buf += "SolidoInstruction "
        l2_data = l1_keys['SolidoInstruction']
        if 'DeactivateValidator' in l2_data.keys():
            output_buf += "DeactivateValidator\n"
            trans_data = l2_data['DeactivateValidator']
            output_buf += CheckField(trans_data, 'solido_instance')
            output_buf += CheckField(trans_data, 'manager')
            output_buf += CheckVoteAccField(trans_data, 'validator_vote_account')
        elif 'AddValidator' in l2_data.keys():
            output_buf += "AddValidator\n"
            trans_data = l2_data['AddValidator']
            output_buf += CheckField(trans_data, 'solido_instance')
            output_buf += CheckField(trans_data, 'manager')
            output_buf += CheckVoteAccField(trans_data, 'validator_vote_account')
        elif 'MigrateStateToV2' in l2_data.keys():
            output_buf += "MigrateStateToV2\n"
            trans_data = l2_data.get('MigrateStateToV2')
            output_buf += CheckField(trans_data, 'solido_instance')
            output_buf += CheckField(trans_data, 'manager')
            output_buf += CheckField(trans_data, 'validator_list')
            output_buf += CheckField(trans_data, 'maintainer_list')
            output_buf += CheckField(trans_data, 'developer_account')
            output_buf += CheckField(trans_data, 'max_maintainers')
            output_buf += CheckField(trans_data, 'max_validators')
            output_buf += CheckField(trans_data, 'max_commission_percentage')

            reward_distribution = trans_data.get('reward_distribution')
            output_buf += CheckRewardField(reward_distribution, 'treasury_fee')
            output_buf += CheckRewardField(reward_distribution, 'developer_fee')
            output_buf += CheckRewardField(reward_distribution, 'st_sol_appreciation')
        else:
            output_buf += "Unknown instruction\n"
            VerificationStatus = False
    elif 'BpfLoaderUpgrade' in l1_keys.keys():
        output_buf += "BpfLoaderUpgrade\n"
        l2_data = l1_keys['BpfLoaderUpgrade']
        output_buf += CheckField(l2_data, 'program_to_upgrade')
        output_buf += CheckField(l2_data, 'program_data_address')
        output_buf += CheckField(l2_data, 'buffer_address')
    else:
        output_buf += "Unknown instruction\n"
        VerificationStatus = False

    output_buf += "\n"
    print(output_buf)
    return VerificationStatus
