// Copyright 2020 The Exonum Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Cryptocurrency database schema.

use exonum::{
    crypto::Hash,
    merkledb::{
        access::{Access, FromAccess, RawAccessMut},
        Group, ObjectHash, ProofListIndex, RawProofMapIndex,
    },
    runtime::CallerAddress as Address,
};
use exonum_derive::{FromAccess, RequireArtifact};

use crate::{wallet::Wallet, INITIAL_BALANCE};
use crate::{transactions::TxSendApprove};

/// Database schema for the cryptocurrency.
///
/// Note that the schema is crate-private, but it has a public part.
#[derive(Debug, FromAccess)]
pub(crate) struct SchemaImpl<T: Access> {
    /// Public part of the schema.
    #[from_access(flatten)]
    pub public: Schema<T>,
    /// History for specific wallets.
    pub wallet_history: Group<T, Address, ProofListIndex<T::Base, Hash>>,
}

/// Public part of the cryptocurrency schema.
#[derive(Debug, FromAccess, RequireArtifact)]
#[require_artifact(name = "exonum-cryptocurrency")]
pub struct Schema<T: Access> {
    /// Map of wallet keys to information about the corresponding account.
    pub wallets: RawProofMapIndex<T::Base, Address, Wallet>,
    /// Map of approval transactions hash to infromation about the corresponding approval transaction
    pub approval_transactions: RawProofMapIndex<T::Base, Hash, TxSendApprove>
}

impl<T: Access> SchemaImpl<T> {
    pub fn new(access: T) -> Self {
        Self::from_root(access).unwrap()
    }

    pub fn wallet(&self, address: Address) -> Option<Wallet> {
        self.public.wallets.get(&address)
    }
}

impl<T> SchemaImpl<T>
where
    T: Access,
    T::Base: RawAccessMut,
{
    /// Increases balance of the wallet and append new record to its history.
    pub fn increase_wallet_balance(&mut self, wallet: Wallet, amount: u64, transaction: Hash) {
        let mut history = self.wallet_history.get(&wallet.owner);
        history.push(transaction);
        let history_hash = history.object_hash();
        let balance = wallet.balance;
        let wallet = wallet.set_balance(balance + amount, &history_hash);
        let wallet_key = wallet.owner;
        self.public.wallets.put(&wallet_key, wallet);
    }

    /// Decreases balance of the wallet and append new record to its history.
    pub fn decrease_wallet_balance(&mut self, wallet: Wallet, amount: u64, transaction: Hash) {
        let mut history = self.wallet_history.get(&wallet.owner);
        history.push(transaction);
        let history_hash = history.object_hash();
        let balance = wallet.balance;
        let wallet = wallet.set_balance(balance - amount, &history_hash);
        let wallet_key = wallet.owner;
        self.public.wallets.put(&wallet_key, wallet);
    }

    /// Creates a new wallet and append first record to its history.
    pub fn create_wallet(&mut self, key: Address, name: &str, transaction: Hash) {
        let mut history = self.wallet_history.get(&key);
        history.push(transaction);
        let history_hash = history.object_hash();
        let wallet = Wallet::new(key, name, INITIAL_BALANCE, 0,history.len(), &history_hash);
        self.public.wallets.put(&key, wallet);
    }

    /// Increases freezed balance of the wallet and append new record to its history.
    pub fn change_wallet_balance_freezed(&mut self, wallet: Wallet, amount: u64, transaction: Hash) {
        let mut history = self.wallet_history.get(&wallet.owner);
        history.push(transaction);

        let history_hash_increase = history.object_hash();
        let balance_freezed = wallet.balance_freezed;
        let wallet = wallet.set_balance_freezed(balance_freezed + amount, &history_hash_increase);

        let wallet_key = wallet.owner;
        self.public.wallets.put(&wallet_key, wallet);
    }

    /// Append new unapproved transaction record to db.
    /// 'wallet' - wallet of sender
    pub fn create_approve_transaction(&mut self, wallet: Wallet, amount: u64, to: Address, approver: Address, tx_hash: Hash) {
        // Update freezed balance & save the history
        self.change_wallet_balance_freezed(wallet, amount, tx_hash);

        // Save transaction in schema.approval_transactions
        let transaction = TxSendApprove::new(to, amount, approver);
        self.public.approval_transactions.put(&tx_hash, transaction);
    }
}
