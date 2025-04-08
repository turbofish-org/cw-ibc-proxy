#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Binary, CosmosMsg, Deps, DepsMut, Env, IbcMsg, IbcTimeout, MessageInfo,
    Response, StdResult,
};
use cw2::set_contract_version;

use crate::admin::Admin;
use crate::error::ContractError;
use crate::msg::{ExecuteMsg, GetBalanceResponse, InstantiateMsg, QueryMsg};
use crate::state::{State, STATE};

const CONTRACT_NAME: &str = "crates.io:osmosis-revenue-transfer-contract";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let state = State {
        channel_id: msg.channel_id.clone(),
        ibc_timeout_interval: msg.ibc_timeout_interval,
        min_disbursal_amount: msg.min_disbursal_amount,
        memo: msg.memo.clone(),
        to_address: msg.to_address.clone(),
        admin: Admin::Settled {
            current: deps.api.addr_validate(&msg.admin)?,
        },
    };
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    STATE.save(deps.storage, &state)?;

    Ok(Response::new().add_attribute("method", "instantiate"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::DisburseFunds { denom } => execute::disburse_funds(deps, env, denom),
        ExecuteMsg::SwapExactAmountIn {
            routes,
            token_in,
            token_out_min_amount,
        } => execute::swap_exact_amount_in(env, deps, info, routes, token_in, token_out_min_amount),
        ExecuteMsg::TransferAdmin { to } => execute::transfer_admin(deps, info, to),
        ExecuteMsg::CancelTransferAdmin {} => execute::cancel_transfer_admin(deps, info),
        ExecuteMsg::ClaimAdmin {} => execute::claim_admin(deps, info),
    }
}

pub mod execute {
    use cosmwasm_std::{ensure_eq, AnyMsg, Coin, Empty};
    use osmosis_std::types::osmosis::poolmanager::v1beta1::{
        MsgSwapExactAmountIn, SwapAmountInRoute,
    };

    use super::*;

    pub fn disburse_funds(
        deps: DepsMut,
        env: Env,
        denom: String,
    ) -> Result<Response, ContractError> {
        let state = STATE.load(deps.storage)?;
        let coin = deps.querier.query_balance(env.contract.address, denom)?;

        if coin.amount < state.min_disbursal_amount.into() {
            return Err(ContractError::InsufficientFunds {});
        }

        let time = env.block.time;
        let timeout_timestamp = time.plus_seconds(state.ibc_timeout_interval);

        let msg = CosmosMsg::Ibc(IbcMsg::Transfer {
            channel_id: state.channel_id,
            to_address: state.to_address,
            amount: coin,
            timeout: IbcTimeout::with_timestamp(timeout_timestamp),
            memo: Some(state.memo),
        });

        Ok(Response::new()
            .add_attribute("method", "disburse_funds")
            .add_message(msg))
    }

    /// Swap on behalf of the contract.
    pub fn swap_exact_amount_in(
        env: Env,
        deps: DepsMut,
        info: MessageInfo,
        routes: Vec<SwapAmountInRoute>,
        token_in: Coin,
        token_out_min_amount: String,
    ) -> Result<Response, ContractError> {
        let state = STATE.load(deps.storage)?;

        ensure_eq!(
            info.sender,
            state.admin.admin(),
            ContractError::Unauthorized {}
        );

        let msg = CosmosMsg::<Empty>::Any(AnyMsg {
            type_url: MsgSwapExactAmountIn::TYPE_URL.to_string(),
            value: Binary::new(
                MsgSwapExactAmountIn {
                    sender: env.contract.address.to_string(),
                    routes,
                    token_in: Some(token_in.into()),
                    token_out_min_amount,
                }
                .to_proto_bytes(),
            ),
        });

        Ok(Response::new()
            .add_attribute("method", "swap_exact_amount_in")
            .add_message(msg))
    }

    pub fn transfer_admin(
        deps: DepsMut,
        info: MessageInfo,
        to: String,
    ) -> Result<Response, ContractError> {
        STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
            state.admin = state
                .admin
                .transfer(&info.sender, deps.api.addr_validate(&to)?)?;
            Ok(state)
        })?;

        Ok(Response::new().add_attribute("method", "transfer_admin"))
    }

    pub fn cancel_transfer_admin(
        deps: DepsMut,
        info: MessageInfo,
    ) -> Result<Response, ContractError> {
        STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
            state.admin = state.admin.cancel_transfer(&info.sender)?;
            Ok(state)
        })?;

        Ok(Response::new().add_attribute("method", "cancel_transfer_admin"))
    }

    pub fn claim_admin(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
        STATE.update(deps.storage, |mut state| -> Result<_, ContractError> {
            state.admin = state.admin.claim(&info.sender)?;
            Ok(state)
        })?;

        Ok(Response::new().add_attribute("method", "claim_admin"))
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetBalance { denom } => to_json_binary(&query::balance(deps, env, denom)?),
    }
}

pub mod query {
    use super::*;

    pub fn balance(deps: Deps, env: Env, denom: String) -> StdResult<GetBalanceResponse> {
        let coin = deps.querier.query_balance(env.contract.address, denom)?;

        Ok(GetBalanceResponse { balance: coin })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{
        message_info, mock_dependencies, mock_dependencies_with_balance, mock_env,
        MOCK_CONTRACT_ADDR,
    };
    use cosmwasm_std::{coins, from_json, Addr, Coin, Uint128};
    use osmosis_std::types::osmosis::poolmanager::v1beta1::{
        MsgSwapExactAmountIn, SwapAmountInRoute,
    };

    #[test]
    fn disburse_funds() {
        let mut deps = mock_dependencies_with_balance(&[Coin {
            amount: Uint128::new(2000),
            denom: "denom".to_string(),
        }]);

        let msg = InstantiateMsg {
            min_disbursal_amount: 0,
            channel_id: "channel-0".to_string(),
            ibc_timeout_interval: 1000,
            memo: "memo".to_string(),
            to_address: "to_address".to_string(),
            admin: deps.api.addr_make("admin").to_string(),
        };

        let info = message_info(&Addr::unchecked("123"), &coins(2000, "denom"));
        let _res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
        let msg = ExecuteMsg::DisburseFunds {
            denom: "denom".to_string(),
        };
        let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    }

    #[test]
    fn swap_exact_amount_in() {
        let mut deps = mock_dependencies_with_balance(&[Coin {
            amount: Uint128::new(2000),
            denom: "denom".to_string(),
        }]);

        let admin = deps.api.addr_make("admin");

        let msg = InstantiateMsg {
            min_disbursal_amount: 0,
            channel_id: "channel-0".to_string(),
            ibc_timeout_interval: 1000,
            memo: "memo".to_string(),
            to_address: "to_address".to_string(),
            admin: admin.to_string(),
        };
        let info = message_info(&admin, &coins(2000, "denom"));
        let _res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let routes = vec![
            SwapAmountInRoute {
                pool_id: 1,
                token_out_denom: "denom2".to_string(),
            },
            SwapAmountInRoute {
                pool_id: 2,
                token_out_denom: "denom3".to_string(),
            },
        ];
        let token_in = Coin {
            amount: Uint128::new(1000),
            denom: "denom1".to_string(),
        };
        let token_out_min_amount = "2000".to_string();

        let msg = ExecuteMsg::SwapExactAmountIn {
            routes: routes.clone(),
            token_in: token_in.clone(),
            token_out_min_amount: token_out_min_amount.clone(),
        };
        let res = execute(deps.as_mut(), mock_env(), info, msg.clone()).unwrap();

        assert_eq!(res.messages.len(), 1);

        let CosmosMsg::Any(any) = res.messages[0].clone().msg else {
            panic!("Expected a CosmosMsg::Any");
        };

        assert_eq!(
            MsgSwapExactAmountIn::try_from(any.value).unwrap(),
            MsgSwapExactAmountIn {
                sender: MOCK_CONTRACT_ADDR.to_string(),
                routes,
                token_in: Some(token_in.into()),
                token_out_min_amount,
            }
        );

        // non-admin can't perform the swap
        let info = message_info(&Addr::unchecked("123"), &coins(2000, "denom"));
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(res, ContractError::Unauthorized {});
    }

    #[test]
    fn balances() {
        let mut deps = mock_dependencies_with_balance(&[Coin {
            amount: Uint128::new(2000),
            denom: "denom".to_string(),
        }]);

        let msg = InstantiateMsg {
            min_disbursal_amount: 0,
            channel_id: "channel-0".to_string(),
            ibc_timeout_interval: 1000,
            memo: "memo".to_string(),
            to_address: "to_address".to_string(),
            admin: deps.api.addr_make("admin").to_string(),
        };
        let info = message_info(&Addr::unchecked("123"), &coins(2000, "denom"));
        let _res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
        let msg = QueryMsg::GetBalance {
            denom: "denom".to_string(),
        };
        let res = query(deps.as_ref(), mock_env(), msg).unwrap();

        let value: GetBalanceResponse = from_json(&res).unwrap();
        assert_eq!(
            Coin {
                amount: Uint128::new(2000),
                denom: "denom".to_string(),
            },
            value.balance
        );
    }

    #[test]
    fn test_admin() {
        let mut deps = mock_dependencies();
        let admin = deps.api.addr_make("admin");
        let to = deps.api.addr_make("to");

        let msg = InstantiateMsg {
            min_disbursal_amount: 0,
            channel_id: "channel-0".to_string(),
            ibc_timeout_interval: 1000,
            memo: "memo".to_string(),
            to_address: "to_address".to_string(),
            admin: admin.to_string(),
        };
        let info = message_info(&admin, &[]);
        let _res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(
            state.admin,
            Admin::Settled {
                current: admin.clone()
            }
        );

        // transfer admin and cancel transfer
        let msg = ExecuteMsg::TransferAdmin { to: to.to_string() };
        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(
            state.admin,
            Admin::Transferring {
                from: admin.clone(),
                to: to.clone()
            }
        );

        let msg = ExecuteMsg::CancelTransferAdmin {};
        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(
            state.admin,
            Admin::Settled {
                current: admin.clone()
            }
        );

        // transfer admin and claim
        let msg = ExecuteMsg::TransferAdmin { to: to.to_string() };
        let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(
            state.admin,
            Admin::Transferring {
                from: admin.clone(),
                to: to.clone()
            }
        );

        let info = message_info(&to, &[]);
        let msg = ExecuteMsg::ClaimAdmin {};
        let _res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        let state = STATE.load(&deps.storage).unwrap();
        assert_eq!(
            state.admin,
            Admin::Settled {
                current: to.clone()
            }
        );

        // prev admin can't transfer anymore
        let info = message_info(&admin, &[]);
        let msg = ExecuteMsg::TransferAdmin { to: to.to_string() };
        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(res, ContractError::Unauthorized {});
    }
}
