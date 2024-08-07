use anchor_lang::prelude::*;
use anchor_lang::solana_program::native_token::LAMPORTS_PER_SOL;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{transfer, Mint, Token, TokenAccount, Transfer},
};

declare_id!("97PknNK44vrCKs88WpEXQwDqhRFS4SZwVVWyo2BjFstn");

const CREATOR: &str = "oggzGFTgRM61YmhEbgWeivVmQx8bSAdBvsPGqN3ZfxN";
const DAY_SECONDS: u64 = 86400;
// redeploy with new creator, withraw program token function, and owner param
#[program]
pub mod test {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        ctx.accounts.global_account.epoch = 0;
        ctx.accounts.global_account.epoch_end = 0;
        ctx.accounts.global_account.token_decimals = ctx.accounts.mint.decimals as u64;
        ctx.accounts.global_account.reward = 0;
        ctx.accounts.global_account.epochs_per_day = 100; // epochs per day
        ctx.accounts.global_account.epoch_reward_percent = 2;
        ctx.accounts.global_account.fee_lamports = LAMPORTS_PER_SOL / 1000000;
        Ok(())
    }
    pub fn initialize_epoch(_ctx: Context<InitializeEpoch>, epoch: u64) -> Result<()> {
        if epoch != 0 {
            return Err(CustomError::InvalidEpoch.into());
        }
        Ok(())
    }
    pub fn change_global_parameters(
        ctx: Context<ChangeGlobalParameters>,
        epoch_reward_percent: u64,
        epochs_per_day: u64,
        fee_lamports: u64,
    ) -> Result<()> {
        if CREATOR.parse::<Pubkey>().unwrap() != ctx.accounts.signer.key() {
            return Err(CustomError::WrongSigner.into());
        }
        ctx.accounts.global_account.epoch_reward_percent = epoch_reward_percent;
        ctx.accounts.global_account.epochs_per_day = epochs_per_day;
        ctx.accounts.global_account.fee_lamports = fee_lamports;
        Ok(())
    }
    pub fn fund_program_token(ctx: Context<FundProgramToken>, amount: u64) -> Result<()> {
        transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.signer_token_account.to_account_info(),
                    to: ctx.accounts.program_token_account.to_account_info(),
                    authority: ctx.accounts.signer.to_account_info(),
                },
            ),
            amount,
        )?;
        Ok(())
    }
    pub fn withdraw_program_token(ctx: Context<WithdrawProgramToken>, amount: u64) -> Result<()> {
        if CREATOR.parse::<Pubkey>().unwrap() != ctx.accounts.signer.key() {
            return Err(CustomError::WrongSigner.into());
        }
        transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.program_token_account.to_account_info(),
                    to: ctx.accounts.signer_token_account.to_account_info(),
                    authority: ctx.accounts.program_authority.to_account_info(),
                },
                &[&[b"auth", &[ctx.bumps.program_authority]]],
            ),
            amount,
        )
    }
    pub fn withdraw_fees(ctx: Context<WithdrawFees>) -> Result<()> {
        if CREATOR.parse::<Pubkey>().unwrap() != ctx.accounts.signer.key() {
            return Err(CustomError::WrongSigner.into());
        }
        let min_rent = Rent::get()?.minimum_balance(8) + 1;
        let transfer = ctx.accounts.program_authority.get_lamports() - min_rent;
        msg!(
            "{}, {}, {}",
            min_rent,
            transfer,
            ctx.accounts.program_authority.get_lamports()
        );
        if transfer <= 0 {
            return Err(CustomError::NoFeesToWithdraw.into());
        }
        **ctx.accounts.program_authority.try_borrow_mut_lamports()? -= transfer;
        **ctx.accounts.signer.try_borrow_mut_lamports()? += transfer;
        Ok(())
    }
    pub fn new_epoch(ctx: Context<NewEpoch>, epoch: u64) -> Result<()> {
        let time = Clock::get()?.unix_timestamp as u64;
        if time < ctx.accounts.global_account.epoch_end {
            return Err(CustomError::EpochNotOver.into());
        }
        if epoch != ctx.accounts.global_account.epoch + 1 {
            return Err(CustomError::WrongEpochProvided.into());
        }
        ctx.accounts.global_account.epoch += 1;
        let unit = DAY_SECONDS / ctx.accounts.global_account.epochs_per_day;
        let units_since_epoch = time / unit;
        ctx.accounts.global_account.epoch_end = (units_since_epoch + 1) * unit;
        ctx.accounts.epoch_account.total_miners = 0;
        // sets the total reward to be balance of holder account / 100 * 2
        ctx.accounts.prev_epoch_account.reward = ctx.accounts.program_token_account.amount / 100 * ctx.accounts.global_account.epoch_reward_percent;
        ctx.accounts.epoch_account.reward = 0;
        // ctx.accounts.epoch_account.reward = ctx.accounts.program_token_account.amount / 100 * ctx.accounts.global_account.epoch_reward_percent;
        ctx.accounts.global_account.reward = ctx.accounts.epoch_account.reward;
        Ok(())
    }
    pub fn mine(ctx: Context<Mine>, epoch: u64) -> Result<()> {
        let time = Clock::get()?.unix_timestamp as u64;
        if time >= ctx.accounts.global_account.epoch_end {
            return Err(CustomError::EpochOver.into());
        }
        if epoch != ctx.accounts.global_account.epoch {
            return Err(CustomError::WrongEpochProvided.into());
        }
        let price = ctx.accounts.global_account.fee_lamports * ctx.accounts.epoch_account.total_miners.pow(2); // y (price) = .1 SOL / 2000 * x ** 2 (minters);
        ctx.accounts.epoch_account.total_miners += 1;
        anchor_lang::system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                anchor_lang::system_program::Transfer {
                    from: ctx.accounts.signer.to_account_info(),
                    to: ctx.accounts.program_authority.to_account_info(),
                },
            ),
            price,
        )?;
        ctx.accounts.mine_account.owner = ctx.accounts.signer.key();
        ctx.accounts.mine_account.epoch = epoch;
        ctx.accounts.mine_data.epochs += 1;
        ctx.accounts.mine_data.owner = ctx.accounts.signer.key();
        Ok(())
    }
    pub fn claim(ctx: Context<Claim>, epoch: u64) -> Result<()> {
        if epoch >= ctx.accounts.global_account.epoch {
            return Err(CustomError::InvalidEpoch.into());
        }
        // if epoch within 10 of current epoch, send user tokens
        // else fail silently, closing their account and incrementing missed
        if epoch <= 10 || epoch >= ctx.accounts.global_account.epoch - 10 {
            let reward =
                ctx.accounts.epoch_account.reward / ctx.accounts.epoch_account.total_miners;
            transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.program_authority.to_account_info(),
                    Transfer {
                        from: ctx.accounts.program_token_account.to_account_info(),
                        to: ctx.accounts.signer_token_account.to_account_info(),
                        authority: ctx.accounts.program_authority.to_account_info(),
                    },
                    &[&[b"auth", &[ctx.bumps.program_authority]]],
                ),
                reward,
            )?;
            ctx.accounts.mine_data.claimed += reward;
        } else {
            ctx.accounts.mine_data.missed += 1;
        }
        Ok(())
    }
}
#[error_code]
pub enum CustomError {
    #[msg("Epoch not over")]
    EpochNotOver,
    #[msg("Wrong epoch provided")]
    WrongEpochProvided,
    #[msg("Epoch over")]
    EpochOver,
    #[msg("Wrong signer")]
    WrongSigner,
    #[msg("Invalid epoch")]
    InvalidEpoch,
    #[msg("No fees to withdraw")]
    NoFeesToWithdraw,
}
#[account]
pub struct GlobalDataAccount {
    pub epoch: u64,
    pub epoch_end: u64,
    pub token_decimals: u64,
    pub reward: u64,
    pub epochs_per_day: u64,
    pub epoch_reward_percent: u64,
    pub fee_lamports: u64,
}
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
    pub mint: Account<'info, Mint>,
    #[account(
        init,
        payer = signer,
        seeds = [b"token_account"],
        bump,
        token::mint = mint,
        token::authority = program_authority
    )]
    pub program_token_account: Account<'info, TokenAccount>,
    #[account(
        init,
        payer = signer,
        seeds = [b"auth"],
        bump,
        space = 8,
    )]
    /// CHECK:
    pub program_authority: AccountInfo<'info>,
    #[account(
        init,
        payer = signer,
        seeds = [b"global"],
        bump,
        space = 8 + 8 + 8 + 8 + 8 + 8 + 8 + 8
    )]
    pub global_account: Account<'info, GlobalDataAccount>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}
#[derive(Accounts)]
#[instruction(epoch: u64)]
pub struct InitializeEpoch<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
    #[account(
        init,
        seeds = [b"epoch", epoch.to_le_bytes().as_ref()],
        bump,
        payer = signer,
        space = 8 + 8 + 8
    )]
    pub epoch_account: Account<'info, EpochAccount>,
    pub system_program: Program<'info, System>,
}
#[derive(Accounts)]
pub struct ChangeGlobalParameters<'info> {
    pub signer: Signer<'info>,
    #[account(
        mut,
        seeds = [b"global"],
        bump,
    )]
    pub global_account: Account<'info, GlobalDataAccount>,
}
#[derive(Accounts)]
pub struct FundProgramToken<'info> {
    pub signer: Signer<'info>,
    #[account(mut)]
    pub signer_token_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        seeds = [b"token_account"],
        bump,
    )]
    pub program_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}
#[derive(Accounts)]
pub struct WithdrawFees<'info> {
    #[account(
        mut,
        seeds = [b"auth"],
        bump,
    )]
    /// CHECK:
    pub program_authority: AccountInfo<'info>,
    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>,
}
#[derive(Accounts)]
pub struct WithdrawProgramToken<'info> {
    pub signer: Signer<'info>,
    #[account(mut)]
    pub signer_token_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        seeds = [b"token_account"],
        bump,
    )]
    pub program_token_account: Account<'info, TokenAccount>,
    #[account(
        seeds = [b"auth"],
        bump,
    )]
    /// CHECK:
    pub program_authority: AccountInfo<'info>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}
#[account]
pub struct EpochAccount {
    pub total_miners: u64,
    pub reward: u64,
}
#[derive(Accounts)]
#[instruction(epoch: u64)]
pub struct NewEpoch<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
    #[account(
        mut,
        seeds = [b"global"],
        bump,
    )]
    pub global_account: Account<'info, GlobalDataAccount>,
    #[account(
        mut,
        seeds = [b"epoch", (epoch - 1).to_le_bytes().as_ref()],
        bump,
    )]
    pub prev_epoch_account: Account<'info, EpochAccount>,
    #[account(
        init,
        payer = signer,
        seeds = [b"epoch", epoch.to_le_bytes().as_ref()],
        bump,
        space = 8 + 8 + 8,
    )]
    pub epoch_account: Account<'info, EpochAccount>,
    #[account(
        seeds = [b"token_account"],
        bump,
    )]
    pub program_token_account: Account<'info, TokenAccount>,
    pub system_program: Program<'info, System>,
}
#[account]
pub struct MineData {
    claimed: u64,
    epochs: u64,
    missed: u64,
    owner: Pubkey,
}
#[account]
pub struct MineAccount {
    owner: Pubkey,
    epoch: u64,
}
#[derive(Accounts)]
#[instruction(epoch: u64)]
pub struct Mine<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
    #[account(
        init,
        seeds = [b"mine", signer.key().as_ref(), epoch.to_le_bytes().as_ref()],
        bump,
        payer = signer,
        space = 8 + 32 + 8,
    )]
    pub mine_account: Account<'info, MineAccount>,
    #[account(
        init_if_needed,
        seeds = [b"mine_data", signer.key().as_ref()],
        bump,
        payer = signer,
        space = 8 + 8 + 8 + 8 + 32
    )]
    pub mine_data: Account<'info, MineData>,
    #[account(
        mut,
        seeds = [b"epoch", epoch.to_le_bytes().as_ref()],
        bump,
    )]
    pub epoch_account: Account<'info, EpochAccount>,
    #[account(
        seeds = [b"global"],
        bump
    )]
    pub global_account: Account<'info, GlobalDataAccount>,
    #[account(
        mut,
        seeds = [b"auth"],
        bump,
    )]
    /// CHECK:
    pub program_authority: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}
#[derive(Accounts)]
#[instruction(epoch: u64)]
pub struct Claim<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
    pub mint: Account<'info, Mint>,
    #[account(
        mut,
        seeds = [b"mine", signer.key().as_ref(), epoch.to_le_bytes().as_ref()],
        bump,
        close = signer,
    )]
    pub mine_account: Account<'info, MineAccount>,
    #[account(
        mut,
        seeds = [b"mine_data", signer.key().as_ref()],
        bump,
    )]
    pub mine_data: Account<'info, MineData>,
    #[account(
        init_if_needed,
        payer = signer,
        associated_token::mint = mint,
        associated_token::authority = signer
    )]
    pub signer_token_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        seeds = [b"token_account"],
        bump,
    )]
    pub program_token_account: Account<'info, TokenAccount>,
    #[account(
        seeds = [b"auth"],
        bump,
    )]
    /// CHECK:
    pub program_authority: AccountInfo<'info>,
    #[account(
        seeds = [b"epoch", epoch.to_le_bytes().as_ref()],
        bump,
    )]
    pub epoch_account: Account<'info, EpochAccount>,
    #[account(
        seeds = [b"global"],
        bump,
    )]
    pub global_account: Account<'info, GlobalDataAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}
