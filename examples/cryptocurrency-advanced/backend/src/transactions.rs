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

//! Cryptocurrency transactions.

use exonum::{
    crypto::Hash,
    runtime::{CallerAddress as Address, CommonError, ExecutionContext, ExecutionError},
};
use exonum_derive::{exonum_interface, interface_method, BinaryValue, ExecutionFail, ObjectHash};
use exonum_proto::ProtobufConvert;

use crate::{proto, schema::SchemaImpl, CryptocurrencyService};

use rand::Rng;

/// Error codes emitted by wallet transactions during execution.
#[derive(Debug, ExecutionFail)]
pub enum Error {
    /// Wallet already exists.
    ///
    /// Can be emitted by `CreateWallet`.
    WalletAlreadyExists = 0,
    /// Sender doesn't exist.
    ///
    /// Can be emitted by `Transfer`.
    SenderNotFound = 1,
    /// Receiver doesn't exist.
    ///
    /// Can be emitted by `Transfer` or `Issue`.
    ReceiverNotFound = 2,
    /// Approver not found
    ///
    /// Can be emitted by TxSendApprove
    ApproverNotFound = 3,
    /// Insufficient currency amount.
    ///
    /// Can be emitted by `Transfer`.
    InsufficientCurrencyAmount = 4,
    /// Sender are same as receiver.
    ///
    /// Can be emitted by 'Transfer`.
    SenderSameAsReceiver = 5,
}

/// Transfer `amount` of the currency from one wallet to another.
#[derive(Clone, Debug)]
#[derive(ProtobufConvert, BinaryValue, ObjectHash)]
#[protobuf_convert(source = "proto::Transfer", serde_pb_convert)]
pub struct Transfer {
    /// Address of receiver's wallet.
    pub to: Address,
    /// Amount of currency to transfer.
    pub amount: u64,
    /// Auxiliary number to guarantee [non-idempotence][idempotence] of transactions.
    ///
    /// [idempotence]: https://en.wikipedia.org/wiki/Idempotence
    pub seed: u64,
}

/// Issue `amount` of the currency to the `wallet`.
#[derive(Clone, Debug)]
#[derive(Serialize, Deserialize)]
#[derive(ProtobufConvert, BinaryValue, ObjectHash)]
#[protobuf_convert(source = "proto::Issue")]
pub struct Issue {
    /// Issued amount of currency.
    pub amount: u64,
    /// Auxiliary number to guarantee [non-idempotence][idempotence] of transactions.
    ///
    /// [idempotence]: https://en.wikipedia.org/wiki/Idempotence
    pub seed: u64,
}

/// Information about transaction with approval stored in the database.
#[derive(Clone, Debug)]
#[derive(ProtobufConvert, BinaryValue, ObjectHash)]
#[protobuf_convert(source = "proto::service::TxSendApprove", serde_pb_convert)]
pub struct TxSendApprove {
    /// Address of receiver's wallet.
    pub to: Address,
    /// Address of approver person
    pub approver: Address,
    /// Amount of currency to transfer.
    pub amount: u64,
    /// Auxiliary number to guarantee [non-idempotence][idempotence] of transactions.
    ///
    /// [idempotence]: https://en.wikipedia.org/wiki/Idempotence
    pub seed: u64,
}

impl TxSendApprove {
    /// Creates a new approval transaction.
    pub fn new(
        to: Address,
        amount: u64,
        approver: Address
    ) -> Self {
        let mut rng = rand::thread_rng();

        Self {
            to,
            amount,
            seed: rng.gen::<u64>(),
            approver
        }
    }
}

/// Create wallet with the given `name`.
#[derive(Clone, Debug)]
#[derive(Serialize, Deserialize)]
#[derive(ProtobufConvert, BinaryValue, ObjectHash)]
#[protobuf_convert(source = "proto::CreateWallet")]
pub struct CreateWallet {
    /// Name of the new wallet.
    pub name: String,
}

impl CreateWallet {
    /// Creates wallet info based on the given `name`.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

/// Cryptocurrency service transactions.
#[exonum_interface]
pub trait CryptocurrencyInterface<Ctx> {
    /// Output returned by the interface methods.
    type Output;

    /// Transfers `amount` of the currency from one wallet to another.
    #[interface_method(id = 0)]
    fn transfer(&self, ctx: Ctx, arg: Transfer) -> Self::Output;
    /// Issues `amount` of the currency to the `wallet`.
    #[interface_method(id = 1)]
    fn issue(&self, ctx: Ctx, arg: Issue) -> Self::Output;
    /// Creates wallet with the given `name`.
    #[interface_method(id = 2)]
    fn create_wallet(&self, ctx: Ctx, arg: CreateWallet) -> Self::Output;
    /// Transfer `amount` of the currency from one wallet to another with approval from third person.
    #[interface_method(id = 3)]
    fn tx_send_approve(&self, ctx: Ctx, arg: TxSendApprove) -> Self::Output;
}

impl CryptocurrencyInterface<ExecutionContext<'_>> for CryptocurrencyService {
    type Output = Result<(), ExecutionError>;

    fn transfer(&self, context: ExecutionContext<'_>, arg: Transfer) -> Self::Output {
        let (from, tx_hash) = extract_info(&context)?;
        let mut schema = SchemaImpl::new(context.service_data());

        let to = arg.to;
        let amount = arg.amount;
        if from == to {
            return Err(Error::SenderSameAsReceiver.into());
        }

        let sender = schema.wallet(from).ok_or(Error::SenderNotFound)?;
        let receiver = schema.wallet(arg.to).ok_or(Error::ReceiverNotFound)?;
        if sender.balance - sender.balance_freezed < amount {
            Err(Error::InsufficientCurrencyAmount.into())
        } else {
            schema.decrease_wallet_balance(sender, amount, tx_hash);
            schema.increase_wallet_balance(receiver, amount, tx_hash);
            Ok(())
        }
    }

    fn issue(&self, context: ExecutionContext<'_>, arg: Issue) -> Self::Output {
        let (from, tx_hash) = extract_info(&context)?;

        let mut schema = SchemaImpl::new(context.service_data());
        if let Some(wallet) = schema.wallet(from) {
            let amount = arg.amount;
            schema.increase_wallet_balance(wallet, amount, tx_hash);
            Ok(())
        } else {
            Err(Error::ReceiverNotFound.into())
        }
    }

    fn create_wallet(&self, context: ExecutionContext<'_>, arg: CreateWallet) -> Self::Output {
        let (from, tx_hash) = extract_info(&context)?;

        let mut schema = SchemaImpl::new(context.service_data());
        if schema.wallet(from).is_none() {
            let name = &arg.name;
            schema.create_wallet(from, name, tx_hash);
            Ok(())
        } else {
            Err(Error::WalletAlreadyExists.into())
        }
    }

    fn tx_send_approve(&self, context: ExecutionContext<'_>, arg: TxSendApprove) -> Self::Output {
        // Getting schema
        let (from, tx_hash) = extract_info(&context)?;
        let mut schema = SchemaImpl::new(context.service_data());

        let to = arg.to;
        let amount = arg.amount;
        if from == to {
            return Err(Error::SenderSameAsReceiver.into());
        }

        // Check sender's waller exists
        let sender_wallet = schema.wallet(from).ok_or(Error::SenderNotFound)?;
        // Check receiver's waller exists
        let _receiver_wallet = schema.wallet(to).ok_or(Error::ReceiverNotFound)?;
        // Check approver's wallet exists
        let _approver_wallet = schema.wallet(arg.approver).ok_or(Error::ApproverNotFound)?;

        // Check balance
        if sender_wallet.balance - sender_wallet.freezed_balance < amount {
            Err(Error::InsufficientCurrencyAmount.into())
        } else {
            schema.create_approve_transaction(sender_wallet, amount, to, arg.approver, tx_hash);
            Ok(())
        }
    }
}

fn extract_info(context: &ExecutionContext<'_>) -> Result<(Address, Hash), ExecutionError> {
    let tx_hash = context
        .transaction_hash()
        .ok_or(CommonError::UnauthorizedCaller)?;
    let from = context.caller().address();
    Ok((from, tx_hash))
}
