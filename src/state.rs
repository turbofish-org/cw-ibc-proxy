use cosmwasm_schema::cw_serde;
use cw_storage_plus::Item;

use crate::admin::Admin;

#[cw_serde]
pub struct State {
    pub channel_id: String,
    pub ibc_timeout_interval: u64,
    pub min_disbursal_amount: u64,
    pub memo: String,
    pub to_address: String,
    pub admin: Admin,
}

pub const STATE: Item<State> = Item::new("state");
