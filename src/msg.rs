use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Coin;
use osmosis_std::types::osmosis::poolmanager::v1beta1::SwapAmountInRoute;

#[cw_serde]
pub struct InstantiateMsg {
    pub min_disbursal_amount: u64,
    pub channel_id: String,
    pub ibc_timeout_interval: u64,
    pub memo: String,
    pub to_address: String,
    pub admin: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    DisburseFunds {
        denom: String,
    },
    SwapExactAmountIn {
        routes: Vec<SwapAmountInRoute>,
        token_in: Coin,
        token_out_min_amount: String,
    },
    TransferAdmin {
        to: String,
    },
    CancelTransferAdmin {},
    ClaimAdmin {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(GetBalanceResponse)]
    GetBalance { denom: String },
}

#[cw_serde]
pub struct GetBalanceResponse {
    pub balance: Coin,
}
