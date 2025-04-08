use cosmwasm_schema::cw_serde;
use cosmwasm_std::{ensure_eq, Addr};

use crate::ContractError;

#[cw_serde]

pub enum Admin {
    Settled { current: Addr },
    Transferring { from: Addr, to: Addr },
}

impl Admin {
    pub fn admin(&self) -> &Addr {
        match self {
            Admin::Settled { current } => current,
            Admin::Transferring { from, to: _ } => from,
        }
    }

    pub fn candidate(&self) -> Option<&Addr> {
        match self {
            Admin::Settled { current: _ } => None,
            Admin::Transferring { from: _, to } => Some(to),
        }
    }

    pub fn transfer(self, sender: &Addr, to: Addr) -> Result<Self, ContractError> {
        ensure_eq!(sender, self.admin(), ContractError::Unauthorized {});

        match self {
            Admin::Settled { current: address } => Ok(Admin::Transferring { from: address, to }),
            Admin::Transferring { from, to: _ } => Ok(Admin::Transferring { from, to }),
        }
    }

    pub fn claim(self, sender: &Addr) -> Result<Self, ContractError> {
        ensure_eq!(
            Some(sender),
            self.candidate(),
            ContractError::Unauthorized {}
        );

        match self {
            Admin::Transferring { from: _, to } => Ok(Admin::Settled { current: to }),
            admin => Ok(admin),
        }
    }

    pub fn cancel_transfer(self, sender: &Addr) -> Result<Self, ContractError> {
        ensure_eq!(sender, self.admin(), ContractError::Unauthorized {});

        match self {
            Admin::Transferring { from, to: _ } => Ok(Admin::Settled { current: from }),
            admin => Ok(admin),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current() {
        let from = Addr::unchecked("from");
        let to = Addr::unchecked("to");

        let admin = Admin::Settled {
            current: from.clone(),
        };

        assert_eq!(admin.admin(), &from);

        let admin = Admin::Transferring {
            from: from.clone(),
            to: to.clone(),
        };

        assert_eq!(admin.admin(), &from);
    }

    #[test]
    fn test_transfer() {
        let from = Addr::unchecked("from");
        let to_1 = Addr::unchecked("to_1");
        let to_2 = Addr::unchecked("to_2");
        let someone_else = Addr::unchecked("someone_else");

        let admin = Admin::Settled {
            current: from.clone(),
        };

        // happy path
        let admin = admin.transfer(&from, to_1.clone()).unwrap();
        assert_eq!(
            admin,
            Admin::Transferring {
                from: from.clone(),
                to: to_1.clone()
            }
        );

        let admin = admin.transfer(&from, to_2.clone()).unwrap();
        assert_eq!(
            admin,
            Admin::Transferring {
                from: from.clone(),
                to: to_2.clone()
            }
        );

        // unhappy path
        let admin = admin.transfer(&someone_else, to_1.clone()).unwrap_err();
        assert_eq!(admin, ContractError::Unauthorized {});
    }

    #[test]
    fn test_claim() {
        let from = Addr::unchecked("from");
        let to = Addr::unchecked("to");
        let someone_else = Addr::unchecked("someone_else");

        let admin = Admin::Settled {
            current: from.clone(),
        };

        let err = admin.claim(&to).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        let admin = Admin::Transferring {
            from: from.clone(),
            to: to.clone(),
        };
        let err = admin.clone().claim(&someone_else).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        let err = admin.clone().claim(&from).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        let admin = admin.claim(&to).unwrap();
        assert_eq!(admin, Admin::Settled { current: to });
    }

    #[test]
    fn test_cancel() {
        let from = Addr::unchecked("from");
        let to = Addr::unchecked("to");
        let someone_else = Addr::unchecked("someone_else");

        let admin = Admin::Transferring {
            from: from.clone(),
            to: to.clone(),
        };

        let err = admin.clone().cancel_transfer(&someone_else).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        let err = admin.clone().cancel_transfer(&to).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        let admin = admin.cancel_transfer(&from).unwrap();
        assert_eq!(admin, Admin::Settled { current: from });
    }
}
