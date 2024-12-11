# options_escrow
# 📜 Options Escrow Service on Solana

devnet:(https://explorer.solana.com/address/9aYFqSL95jbn72YAcdoTXjAiZfwopsV7JhkSsqKLS4cf?cluster=devnet)

This project implements an options escrow service on the Solana blockchain using the Anchor framework. The service allows users to create escrow accounts for options trading, where collateral is locked until the contract conditions are met, ensuring that both parties fulfill their obligations.

## 🛠️ Tech Stack
- **Rust**: The primary language for developing smart contracts on Solana.
- **Anchor**: A framework for Solana smart contract development.
- **Solana**: The blockchain used for high-speed and low-cost transactions.
- **Solana Playground**: An interactive environment to develop and test Solana programs.

## 🎯 Features
- **Option Types**: Supports Call and Put options.
- **Collateral Management**: Allows users to deposit tokens (such as SOL, USDC, or any SPL token) as collateral.
- **Fee System**: A configurable fee system where the fee rate and fee collector can be updated through governance.
- **Governance**: Supports a governance account that controls fee rates and the fee collector's address.
- **Expiration Handling**: Options are settled based on whether they expire In-The-Money (ITM) or Out-Of-The-Money (OTM).
- **Early Exercise**: Supports early exercise for American-style options.

## 📁 Program Structure

### lib.rs Overview

- **Escrow Account**: 
  - Stores details about the option, such as the initializer, option type, strike price, expiration, and collateral.
  
- **Governance**:
  - Stores the fee rate and the fee collector's address.
  - Allows the governance authority to update the protocol fees.

### Key Functions:
- `initialize_escrow`: Initializes the escrow account with the option's parameters.
- `deposit_collateral`: Allows users to deposit collateral into the escrow.
- `settle_escrow`: Settles the option when it expires (based on ITM/OTM).
- `exercise_early`: Allows early exercise for American-style options.
- `update_governance`: Allows the governance authority to update the fee rate and fee collector.
- `transfer_governance`: Transfers the governance authority to another account.
