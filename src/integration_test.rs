use cosmwasm_std::Coin;
use osmosis_std::types::{
    cosmos::bank::v1beta1::{MsgSend, QueryAllBalancesRequest},
    osmosis::poolmanager::v1beta1::SwapAmountInRoute,
};
use osmosis_test_tube::{Account, Bank, Gamm, Module, OsmosisTestApp, Wasm};

use crate::{
    msg::{ExecuteMsg, InstantiateMsg},
    ContractError,
};

macro_rules! assert_contract_error {
    ($actual_err:expr, $expected_err:expr) => {
        assert_eq!(
            $actual_err.to_string(),
            format!(
                "execute error: failed to execute message; message index: 0: {}: execute wasm contract failed",
                $expected_err
            )
        );
    };
}

#[test]
fn test_swap_exact_amount_in() {
    let app = OsmosisTestApp::new();
    let wasm = Wasm::new(&app);
    let bank = Bank::new(&app);
    let gamm = Gamm::new(&app);

    let accounts = app
        .init_accounts(
            &[
                Coin::new(1000000000000000000u128, "denom1"),
                Coin::new(1000000000000000000u128, "denom2"),
                Coin::new(2000000000000000000u128, "denom3"),
                Coin::new(1000000000000000000u128, "uosmo"),
            ],
            4,
        )
        .unwrap();

    let admin = &accounts[0]; // the one managing the contract
    let coin_feeder = &accounts[1]; // the one who feeds the contract with coins
    let pool_creator = &accounts[2]; // the one who creates the pool
    let new_admin = &accounts[3]; // the one who will be the new admin

    let pool_id = gamm
        .create_basic_pool(
            &[
                Coin::new(1000000000000000000u128, "denom1").into(),
                Coin::new(1000000000000000000u128, "denom2").into(),
                Coin::new(2000000000000000000u128, "denom3").into(),
            ],
            &pool_creator,
        )
        .unwrap()
        .data
        .pool_id;

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("wasm32-unknown-unknown")
        .join("release")
        .join("cw_ibc_proxy.wasm");

    let wasm_byte_code = std::fs::read(path).unwrap();
    let code_id = wasm
        .store_code(&wasm_byte_code, None, &admin)
        .unwrap()
        .data
        .code_id;

    let contract_addr = wasm
        .instantiate(
            code_id,
            &InstantiateMsg {
                channel_id: "channel-0".to_string(),
                ibc_timeout_interval: 1000,
                min_disbursal_amount: 100,
                memo: "memo".to_string(),
                to_address: "osmo1qzskhrcjnk5634mq5s5ssdlw34aj844ke4m343".to_string(),
                admin: admin.address().to_string(),
            },
            None,
            Some("proxy"),
            &[],
            &admin,
        )
        .unwrap()
        .data
        .address;

    bank.send(
        MsgSend {
            from_address: coin_feeder.address(),
            to_address: contract_addr.clone(),
            amount: vec![
                Coin::new(1000000000000000000u128, "denom1").into(),
                Coin::new(1000000000000000000u128, "denom2").into(),
            ],
        },
        &coin_feeder,
    )
    .unwrap();

    let balances = bank
        .query_all_balances(&QueryAllBalancesRequest {
            address: contract_addr.to_string(),
            pagination: None,
            resolve_denom: false,
        })
        .unwrap()
        .balances;

    assert_eq!(
        balances,
        vec![
            Coin::new(1000000000000000000u128, "denom1").into(),
            Coin::new(1000000000000000000u128, "denom2").into(),
        ]
    );

    let res = wasm
        .execute(
            &contract_addr,
            &ExecuteMsg::SwapExactAmountIn {
                token_in: Coin::new(1000000000000000000u128, "denom1").into(),
                token_out_min_amount: "100".to_string(),
                routes: vec![SwapAmountInRoute {
                    pool_id,
                    token_out_denom: "denom3".to_string(),
                }],
            },
            &[],
            admin,
        )
        .unwrap();

    let token_out: Coin = res
        .events
        .iter()
        .find(|event| event.ty == "token_swapped")
        .unwrap()
        .attributes
        .iter()
        .find(|attr| attr.key == "tokens_out")
        .unwrap()
        .value
        .clone()
        .parse::<Coin>()
        .unwrap();

    let balances = bank
        .query_all_balances(&QueryAllBalancesRequest {
            address: contract_addr.to_string(),
            pagination: None,
            resolve_denom: false,
        })
        .unwrap()
        .balances;

    assert_eq!(
        balances,
        vec![
            Coin::new(1000000000000000000u128, "denom2").into(),
            token_out.into()
        ]
    );

    // non-admin can't swap
    let res = wasm
        .execute(
            &contract_addr,
            &ExecuteMsg::SwapExactAmountIn {
                token_in: Coin::new(1000000000000000000u128, "denom1").into(),
                token_out_min_amount: "100".to_string(),
                routes: vec![SwapAmountInRoute {
                    pool_id,
                    token_out_denom: "denom3".to_string(),
                }],
            },
            &[],
            &coin_feeder,
        )
        .unwrap_err();

    assert_contract_error!(res, ContractError::Unauthorized {});

    // transfer admin
    wasm.execute(
        &contract_addr,
        &ExecuteMsg::TransferAdmin {
            to: new_admin.address().to_string(),
        },
        &[],
        admin,
    )
    .unwrap();

    let msg = ExecuteMsg::SwapExactAmountIn {
        token_in: Coin::new(10000u128, "denom2").into(),
        token_out_min_amount: "1".to_string(),
        routes: vec![SwapAmountInRoute {
            pool_id,
            token_out_denom: "denom3".to_string(),
        }],
    };

    // new admin can't swap before claim admin rights
    let err = wasm
        .execute(&contract_addr, &msg, &[], new_admin)
        .unwrap_err();

    assert_contract_error!(err, ContractError::Unauthorized {});

    // prev admin can still swap
    wasm.execute(&contract_addr, &msg, &[], admin).unwrap();

    // claim admin rights
    wasm.execute(&contract_addr, &ExecuteMsg::ClaimAdmin {}, &[], new_admin)
        .unwrap();

    // new admin can swap
    wasm.execute(&contract_addr, &msg, &[], new_admin).unwrap();

    // prev admin can't swap anymore
    let err = wasm.execute(&contract_addr, &msg, &[], admin).unwrap_err();
    assert_contract_error!(err, ContractError::Unauthorized {});
}
