use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer, Mint};

declare_id!("9aYFqSL95jbn72YAcdoTXjAiZfwopsV7JhkSsqKLS4cf");

#[program]
mod options_escrow {
    use super::*;

    /// Initializes the escrow account with option parameters and charges a fee.
    ///
    /// The escrow account holds details of the option contract, including the strike price,
    /// expiration date, and the collateral amount. This function also transfers a fee to
    /// the fee collector based on the governance settings.
    pub fn initialize_escrow(
        ctx: Context<InitializeEscrow>,
        option_type: OptionType,      // Type of option: Call or Put
        strike_price: u64,            // Strike price of the option
        expiration: i64,              // Expiration time as a Unix timestamp
        collateral_amount: u64,       // Amount of collateral to be deposited
        collateral_mint: Pubkey,      // Token mint for the collateral
    ) -> Result<()> {
        let escrow_account = &mut ctx.accounts.escrow_account;
        
        // Initialize escrow account details
        escrow_account.initializer_key = *ctx.accounts.initializer.key;
        escrow_account.option_type = option_type;
        escrow_account.strike_price = strike_price;
        escrow_account.expiration = expiration;
        escrow_account.collateral_amount = collateral_amount;
        escrow_account.collateral_mint = collateral_mint;
        escrow_account.is_exercised = false;

        // Transfer fee to the fee collector
        let governance = &ctx.accounts.governance;
        let fee = collateral_amount * governance.fee_rate / 10000; // Calculate fee based on the fee rate
        let cpi_accounts_fee = Transfer {
            from: ctx.accounts.initializer_collateral_account.to_account_info(),
            to: ctx.accounts.fee_collector.to_account_info(),
            authority: ctx.accounts.initializer.to_account_info(),
        };
        let cpi_ctx_fee = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts_fee);
        token::transfer(cpi_ctx_fee, fee)?;

        Ok(())
    }

    /// Deposits collateral into the escrow account.
    ///
    /// This function allows the user to deposit collateral into the escrow account.
    /// It ensures that the correct token type (SPL token) is deposited and verifies
    /// that the user's token account matches the specified collateral mint.
    pub fn deposit_collateral(ctx: Context<DepositCollateral>, amount: u64) -> Result<()> {
        let escrow_account = &ctx.accounts.escrow_account;

        // Ensure the user's collateral account mint matches the escrow's expected mint
        if ctx.accounts.user_collateral_account.mint != escrow_account.collateral_mint {
            return Err(ErrorCode::IncorrectCollateralMint.into());
        }

        // Transfer the collateral from the user's account to the escrow account
        let cpi_accounts = Transfer {
            from: ctx.accounts.user_collateral_account.to_account_info(),
            to: ctx.accounts.escrow_collateral_account.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, amount)?;

        Ok(())
    }

    /// Settles the escrow account upon option expiration and deducts the fee.
    ///
    /// The settlement depends on whether the option expires In-the-Money (ITM) or Out-of-the-Money (OTM).
    /// If ITM, the collateral is transferred to the option holder, minus the governance fee.
    /// If OTM, the collateral is returned to the initializer, also minus the fee.
    pub fn settle_escrow(ctx: Context<SettleEscrow>, is_itm: bool) -> Result<()> {
        let escrow_account = &mut ctx.accounts.escrow_account;
        let governance = &ctx.accounts.governance;

        // Ensure the option has not been exercised yet
        if escrow_account.is_exercised {
            return Err(ErrorCode::OptionAlreadyExercised.into());
        }

        // Ensure the option has expired before settling
        let current_time = Clock::get()?.unix_timestamp;
        if current_time < escrow_account.expiration {
            return Err(ErrorCode::OptionNotExpired.into());
        }

        // Calculate the fee and remaining amount after fee deduction
        let fee = escrow_account.collateral_amount * governance.fee_rate / 10000;
        let amount_after_fee = escrow_account.collateral_amount - fee;

        // Handle the settlement based on whether the option is ITM or OTM
        if is_itm {
            // Transfer collateral (minus fee) to the option holder (user) if ITM
            let cpi_accounts = Transfer {
                from: ctx.accounts.escrow_collateral_account.to_account_info(),
                to: ctx.accounts.user_collateral_account.to_account_info(),
                authority: ctx.accounts.escrow_authority.to_account_info(),
            };
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
            token::transfer(cpi_ctx, amount_after_fee)?;
        } else {
            // Return collateral (minus fee) to the initializer if OTM
            let cpi_accounts = Transfer {
                from: ctx.accounts.escrow_collateral_account.to_account_info(),
                to: ctx.accounts.initializer_collateral_account.to_account_info(),
                authority: ctx.accounts.escrow_authority.to_account_info(),
            };
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
            token::transfer(cpi_ctx, amount_after_fee)?;
        }

        // Transfer the collected fee to the fee collector
        let cpi_accounts_fee = Transfer {
            from: ctx.accounts.escrow_collateral_account.to_account_info(),
            to: ctx.accounts.fee_collector.to_account_info(),
            authority: ctx.accounts.escrow_authority.to_account_info(),
        };
        let cpi_ctx_fee = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts_fee);
        token::transfer(cpi_ctx_fee, fee)?;

        // Mark the option as exercised
        escrow_account.is_exercised = true;
        Ok(())
    }

    /// Allows early exercise of the option for American-style options.
    ///
    /// The option can be exercised early before the expiration if it's an American option.
    /// It follows similar logic as `settle_escrow` to transfer the collateral based on
    /// whether the option is ITM or OTM, and deducts the governance fee.
    pub fn exercise_early(ctx: Context<SettleEscrow>, is_itm: bool) -> Result<()> {
        let escrow_account = &mut ctx.accounts.escrow_account;

        // Ensure the option has not been exercised yet
        if escrow_account.is_exercised {
            return Err(ErrorCode::OptionAlreadyExercised.into());
        }

        // Ensure it's an American option to allow early exercise
        if escrow_account.option_type != OptionType::Call && escrow_account.option_type != OptionType::Put {
            return Err(ErrorCode::CannotExerciseEarly.into());
        }

        // Calculate the fee and remaining amount after fee deduction
        let governance = &ctx.accounts.governance;
        let fee = escrow_account.collateral_amount * governance.fee_rate / 10000;
        let amount_after_fee = escrow_account.collateral_amount - fee;

        // Handle early exercise based on whether the option is ITM or OTM
        if is_itm {
            let cpi_accounts = Transfer {
                from: ctx.accounts.escrow_collateral_account.to_account_info(),
                to: ctx.accounts.user_collateral_account.to_account_info(),
                authority: ctx.accounts.escrow_authority.to_account_info(),
            };
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
            token::transfer(cpi_ctx, amount_after_fee)?;
        } else {
            let cpi_accounts = Transfer {
                from: ctx.accounts.escrow_collateral_account.to_account_info(),
                to: ctx.accounts.initializer_collateral_account.to_account_info(),
                authority: ctx.accounts.escrow_authority.to_account_info(),
            };
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
            token::transfer(cpi_ctx, amount_after_fee)?;
        }

        // Transfer the collected fee to the fee collector
        let cpi_accounts_fee = Transfer {
            from: ctx.accounts.escrow_collateral_account.to_account_info(),
            to: ctx.accounts.fee_collector.to_account_info(),
            authority: ctx.accounts.escrow_authority.to_account_info(),
        };
        let cpi_ctx_fee = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts_fee);
        token::transfer(cpi_ctx_fee, fee)?;

        // Mark the option as exercised
        escrow_account.is_exercised = true;

        Ok(())
    }

    /// Updates governance parameters (fee rate and fee collector).
    ///
    /// This function allows the governance authority to update key parameters, including the
    /// fee rate (as basis points) and the address where protocol fees are collected.
    pub fn update_governance(ctx: Context<UpdateGovernance>, new_fee_rate: u64, new_fee_collector: Pubkey) -> Result<()> {
        let governance = &mut ctx.accounts.governance;
        governance.fee_rate = new_fee_rate;
        governance.fee_collector = new_fee_collector;
        Ok(())
    }

    /// Initializes the governance account.
    ///
    /// This function sets up the governance account, allowing it to store the initial fee rate,
    /// fee collector address, and governance authority responsible for future updates.
    pub fn initialize_governance(ctx: Context<InitializeGovernance>, fee_rate: u64, fee_collector: Pubkey) -> Result<()> {
        let governance = &mut ctx.accounts.governance;
        governance.fee_rate = fee_rate;
        governance.fee_collector = fee_collector;
        governance.governance_authority = *ctx.accounts.governance_authority.key;
        Ok(())
    }

    /// Transfers the governance authority to a new account.
    ///
    /// This function allows the current governance authority to transfer control over the
    /// governance account to a new authority, such as a DAO or multisig.
    pub fn transfer_governance(ctx: Context<UpdateGovernance>, new_governance_authority: Pubkey) -> Result<()> {
        let governance = &mut ctx.accounts.governance;
        governance.governance_authority = new_governance_authority;
        Ok(())
    }
}

#[account]
/// Structure to hold escrow account data.
///
/// This account stores the details of the escrow, such as the initializer (option writer),
/// the type of option (Call or Put), strike price, expiration, collateral amount, and whether
/// the option has been exercised.
pub struct EscrowAccount {
    pub initializer_key: Pubkey,     // The user who initialized the escrow
    pub option_type: OptionType,     // Call or Put option
    pub strike_price: u64,           // Strike price for the option
    pub expiration: i64,             // Expiration time (Unix timestamp)
    pub collateral_amount: u64,      // Collateral amount deposited in the escrow
    pub collateral_mint: Pubkey,     // Token mint for the collateral (SPL token)
    pub is_exercised: bool,          // Indicates if the option has been exercised
}

/// Governance account storing key parameters for the protocol.
///
/// The governance account stores the fee rate (in basis points) for the protocol and the
/// address of the fee collector. It also stores the governance authority, which is allowed
/// to update these parameters.
#[account]
pub struct Governance {
    pub fee_rate: u64,                // Fee rate in basis points (e.g., 500 = 5.00%)
    pub fee_collector: Pubkey,        // Address where protocol fees are collected
    pub governance_authority: Pubkey, // Account authorized to update governance settings
}

/// Enum to define the option type (Call or Put).
///
/// This enum specifies the type of option being created: either a Call option (buy) or a Put option (sell).
#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq)]
pub enum OptionType {
    Call, // Call option gives the buyer the right to buy
    Put,  // Put option gives the buyer the right to sell
}

#[derive(Accounts)]
/// Context for initializing the escrow.
///
/// This struct defines the context for the `initialize_escrow` instruction, specifying
/// the accounts involved, including the escrow account, the initializer, the collateral
/// accounts, and the governance account.
pub struct InitializeEscrow<'info> {
    #[account(init, payer = initializer, space = 8 + 8 + 8 + 8 + 8 + 32 + 1)]
    pub escrow_account: Account<'info, EscrowAccount>,    // Escrow account to store option details
    #[account(mut)]
    pub initializer: Signer<'info>,                      // The initializer (creator of the escrow)
    #[account(mut)]
    pub initializer_collateral_account: Account<'info, TokenAccount>,  // Initializer's token account for collateral
    #[account(mut)]
    pub fee_collector: Account<'info, TokenAccount>,     // Account where protocol fees are sent
    #[account(mut)]
    pub governance: Account<'info, Governance>,          // Governance account storing fee rate and fee collector
    pub system_program: Program<'info, System>,          // System program for account creation
    pub token_program: Program<'info, Token>,            // Token program for handling SPL tokens
    pub rent: Sysvar<'info, Rent>,                       // Rent system for account initialization
}

#[derive(Accounts)]
/// Context for depositing collateral into the escrow.
///
/// This struct defines the context for the `deposit_collateral` instruction, specifying
/// the user's collateral account, the escrow account, and the necessary programs.
pub struct DepositCollateral<'info> {
    #[account(mut)]
    pub escrow_account: Account<'info, EscrowAccount>,    // Escrow account receiving collateral
    #[account(mut)]
    pub user: Signer<'info>,                              // User depositing collateral
    #[account(mut)]
    pub user_collateral_account: Account<'info, TokenAccount>,  // User's token account for depositing collateral
    #[account(mut)]
    pub escrow_collateral_account: Account<'info, TokenAccount>, // Escrow's token account holding collateral
    pub token_program: Program<'info, Token>,             // Token program for token transfers
}

#[derive(Accounts)]
/// Context for settling the escrow when the option expires.
///
/// This struct defines the context for the `settle_escrow` and `exercise_early` instructions,
/// specifying the involved accounts, including the escrow, the user, the initializer, and the
/// governance and fee accounts.
pub struct SettleEscrow<'info> {
    #[account(mut)]
    pub escrow_account: Account<'info, EscrowAccount>,    // Escrow account storing option details
    #[account(mut)]
    pub user: Signer<'info>,                              // The user settling the option
    #[account(mut)]
    pub user_collateral_account: Account<'info, TokenAccount>,  // User's token account (receiving collateral if ITM)
    #[account(mut)]
    pub escrow_collateral_account: Account<'info, TokenAccount>, // Escrow's token account holding collateral
    #[account(mut)]
    pub initializer_collateral_account: Account<'info, TokenAccount>, // Initializer's token account (receiving collateral if OTM)
    #[account(mut)]
    pub escrow_authority: AccountInfo<'info>,             // The authority controlling the escrow (PDA)
    #[account(mut)]
    pub fee_collector: Account<'info, TokenAccount>,      // Account where protocol fees are sent
    #[account(mut)]
    pub governance: Account<'info, Governance>,           // Governance account storing fee rate and fee collector
    pub token_program: Program<'info, Token>,             // Token program for token transfers
}

#[derive(Accounts)]
/// Context for updating governance settings.
///
/// This struct defines the context for the `update_governance` instruction, which
/// allows the governance authority to update the fee rate and fee collector.
pub struct UpdateGovernance<'info> {
    #[account(mut, has_one = governance_authority)]
    pub governance: Account<'info, Governance>,  // Governance account to be updated
    pub governance_authority: Signer<'info>,     // Governance authority account
}

#[derive(Accounts)]
/// Context for initializing the governance account.
///
/// This struct defines the context for the `initialize_governance` instruction, which
/// creates the governance account and sets the initial fee rate and fee collector.
pub struct InitializeGovernance<'info> {
    #[account(init, payer = governance_authority, space = 8 + 32 + 32 + 8)]
    pub governance: Account<'info, Governance>,           // Governance account to store protocol parameters
    #[account(mut)]
    pub governance_authority: Signer<'info>,              // Initial governance authority (e.g., program deployer)
    pub system_program: Program<'info, System>,           // System program for account creation
}

#[error_code]
/// Custom error codes for the program.
///
/// This enum defines custom error codes for common failure cases in the protocol,
/// such as trying to exercise an option that has already been exercised or using
/// the wrong collateral mint.
pub enum ErrorCode {
    #[msg("The option has already been exercised.")]
    OptionAlreadyExercised,
    #[msg("The option has not yet expired.")]
    OptionNotExpired,
    #[msg("Incorrect collateral mint provided.")]
    IncorrectCollateralMint,
    #[msg("Cannot exercise the option early.")]
    CannotExerciseEarly,
}
