use std::{str::FromStr, collections::HashMap};

use serde::{Serialize, Deserialize};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{vote::{instruction::VoteInstruction, self}, signature::Signature, transaction::{VersionedTransaction, SanitizedTransaction}, pubkey::Pubkey};
use solana_transaction_status::{EncodedTransaction, UiTransactionEncoding, UiConfirmedBlock, EncodedConfirmedBlock, TransactionBinaryEncoding, BlockHeader};
use solana_account_decoder::{self, UiAccountData, parse_stake::{parse_stake, StakeAccountType}, parse_vote::parse_vote};
use solana_entry::entry::{Entry, EntrySlice};
use solana_sdk::hash::Hash;

#[macro_export]
macro_rules! send_rpc_call {
    ($url:expr, $body:expr) => {{
        use reqwest::header::{ACCEPT, CONTENT_TYPE};
        let req_client = reqwest::Client::new();

        let res = req_client
            .post($url)
            .body($body)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .send()
            .await
            .expect("error")
            .text()
            .await
            .expect("error");
        res
    }};
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockResponse {
    pub jsonrpc: String,
    pub result: UiConfirmedBlock,
    pub id: i64,
}


async fn get_block(slot: u64, endpoint: String) -> GetBlockResponse { 
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getBlock",
        "params":[
            slot,
            { 
                "encoding": "base58", // better for deserialzing
                "maxSupportedTransactionVersion": 0,
            }
        ]
    }).to_string();
    let resp = send_rpc_call!(endpoint, request);
    let resp = serde_json::from_str::<GetBlockResponse>(&resp).unwrap();
    resp
}

async fn parse_block_votes() { 
    // let endpoint = "http://127.0.0.1:8002";

    let endpoint = "https://rpc.helius.xyz/?api-key=cee342ba-0773-41f7-a6e0-9ff01fff124b";
    let vote_program_id = "Vote111111111111111111111111111111111111111".to_string();
    let vote_program_id = Pubkey::from_str(&vote_program_id).unwrap();

    let client = RpcClient::new(endpoint);
    let vote_accounts = client.get_vote_accounts().unwrap();
    let leader_stakes = vote_accounts.current
        .iter()
        .chain(vote_accounts.delinquent.iter())
        .map(|x| (x.node_pubkey.clone(), x.activated_stake))
        .collect::<HashMap<_, _>>();
    let total_stake = leader_stakes.iter().fold(0, |sum, i| sum + *i.1);

    // let slot = 354;
    let slot = 194458133;
    let resp = get_block(slot, endpoint.to_string()).await;
    let block = resp.result;

    // // doesnt support new version txs 
    // let block = client.get_block(slot).unwrap();
    // println!("{:#?}", block);

    if block.transactions.is_none() { 
        println!("no transactions");
        return;
    }

    for tx in block.transactions.unwrap().iter() {
        let tx = &tx.transaction;
        let tx = match tx { 
            EncodedTransaction::Binary(tx, enc) => {
                assert!(*enc == TransactionBinaryEncoding::Base58);
                let tx = bs58::decode(tx).into_vec().unwrap();
                let tx: VersionedTransaction = bincode::deserialize(&tx[..]).unwrap();
                tx
            }
            _ => panic!("ahh")
        };

        let msg = tx.message;
        if !msg.static_account_keys().contains(&vote_program_id) { 
            println!("tx doesnt include vote program ...");
            continue;
        }

        let ix = msg.instructions().get(0).unwrap();
        let data = &ix.data;
        let vote_ix: VoteInstruction = bincode::deserialize(&data[..]).unwrap();
        let slot_vote = vote_ix.last_voted_slot().unwrap_or_default();
        let bank_hash = match &vote_ix { 
            VoteInstruction::Vote(v) => Some(v.hash),   
            VoteInstruction::CompactUpdateVoteState(v) => Some(v.hash),
            _ => None
        };

        println!("{:?}", vote_ix);
        println!("voted for slot {:?} with bank_hash {:?}", slot_vote, bank_hash);

        let node_pubkey = msg.static_account_keys().get(0).unwrap().to_string();
        let stake_amount = leader_stakes.get(&node_pubkey).unwrap();
        println!("{:?} {:?}", node_pubkey, stake_amount);

        // verify the signature
        let msg_bytes = msg.serialize();
        let sig_verifies: Vec<_> = tx.signatures
            .iter()
            .zip(msg.static_account_keys().iter())
            .map(|(signature, pubkey)| signature.verify(pubkey.as_ref(), &msg_bytes[..]))
            .collect();

        println!("{:?}", sig_verifies);

        break;
    }
}


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockHeadersResponse {
    pub jsonrpc: String,
    pub result: Vec<u8>,
    pub id: i64,
}

async fn get_block_headers(slot: u64, endpoint: String) -> GetBlockHeadersResponse { 
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getBlockHeaders",
        "params":[
            slot
        ]
    }).to_string();
    let resp = send_rpc_call!(endpoint, request);
    let resp = serde_json::from_str::<GetBlockHeadersResponse>(&resp).unwrap();
    resp
}

pub async fn verify_slot() { 
    let endpoint = "http://127.0.0.1:8002";

    let client = RpcClient::new(endpoint);

    let slot = client.get_slot().unwrap();
    println!("verifying slot {:?}", slot);

    let block_headers = get_block_headers(slot, endpoint.to_string()).await.result;
    let block_headers: BlockHeader = bincode::deserialize(&block_headers).unwrap();

    let entries = block_headers.entries; 
    let last_blockhash = block_headers.last_blockhash;
    let verified = entries.verify(&last_blockhash);
    if !verified { 
        println!("entry verification failed ...");
        return;
    }
    println!("entry verification passed!");

}

#[tokio::main]
async fn main() {
    // parse_block_votes().await;
    verify_slot().await;

    // let endpoint = "http://127.0.0.1:8002";

    // // // GPA on stake times out here
    // // let endpoint = "https://rpc.helius.xyz/?api-key=cee342ba-0773-41f7-a6e0-9ff01fff124b";
    // let client = RpcClient::new(endpoint);

    // let vote_accounts = client.get_vote_accounts().unwrap();
    // let leader_stakes = vote_accounts.current
    //     .iter()
    //     .chain(vote_accounts.delinquent.iter())
    //     .map(|x| (x.node_pubkey.clone(), x.activated_stake))
    //     .collect::<HashMap<_, _>>();
    // println!("{:?}", leader_stakes);

    // println!("---");
    // let stake_program = Pubkey::from_str("Stake11111111111111111111111111111111111111").unwrap();
    // let stake_accounts = client.get_program_accounts(&stake_program).unwrap();
    // for (pubkey, account) in stake_accounts.iter() { 
    //     let stake = parse_stake(account.data.as_slice()).unwrap();
    //     match stake {
    //         StakeAccountType::Initialized(stake) => println!("{:?}", stake),
    //         StakeAccountType::Delegated(stake) => println!("{:?}", stake),
    //         _ => {}
    //     }
    // }

    // println!("---");
    // let vote_program = Pubkey::from_str("Vote111111111111111111111111111111111111111").unwrap();
    // let vote_accounts = client.get_program_accounts(&vote_program).unwrap();
    // for (_, account) in vote_accounts.iter() { 
    //     let vote = parse_vote(account.data.as_slice()).unwrap();
    //     println!("{:?}", vote);
    // }
    
    // println!("---");
    // let leader_schedule = client.get_leader_schedule(None).unwrap().unwrap();
    // println!("{:?}", leader_schedule);

    // let slot = 194458133;
    // let leader_schedule = client.get_leader_schedule(Some(slot)).unwrap().unwrap();
    // let leaders = leader_schedule.iter().map(|(pubkey, _)| Pubkey::from_str(pubkey).unwrap()).collect::<Vec<_>>();
    // let stakes = leaders.iter().map(|leader| { 
    //     // todo: get stake account pubkey
    //     let stake = client.get_stake_activation(*leader, None).unwrap();
    //     let stake_amount = stake.active;
    //     stake_amount
    // });

    // let leader_stakes = leaders.iter().zip(stakes).collect::<HashMap<_, _>>();
    // println!("{:#?}", leader_stakes);

}
