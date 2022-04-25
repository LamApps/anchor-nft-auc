use anchor_lang::prelude::*;
use anchor_lang::{AccountDeserialize, AnchorDeserialize};
use anchor_spl::token::{self, TokenAccount, Transfer};

#[program]
mod auction {
    use super::*;

    pub fn create_auction(ctx: Context<CreateAuction>, start_price: u64) -> Result<()> {
        // init auction
        let auction = &mut ctx.accounts.auction;
        auction.ongoing = true;
        auction.seller = *ctx.accounts.seller.key;
        auction.item_holder = *ctx.accounts.item_holder.to_account_info().key;
        auction.currency_holder = *ctx.accounts.currency_holder.to_account_info().key;
        auction.bidder = *ctx.accounts.seller.key;
        auction.price = start_price;
        Ok(())
    }

    pub fn bid(ctx: Context<Bid>, price: u64) -> Result<()> {
        let auction = &mut ctx.accounts.auction;

        // check bid price
        if price <= auction.price {
            return Err(AuctionErr::BidPirceTooLow.into());
        }

        // if refund_receiver exist, return money back to it
        if auction.refund_receiver != Pubkey::default() {
            let (_, seed) =
                Pubkey::find_program_address(&[&auction.seller.to_bytes()], &ctx.program_id);
            let seeds = &[auction.seller.as_ref(), &[seed]];
            let signer = &[&seeds[..]];
            let cpi_accounts = Transfer {
                from: ctx.accounts.currency_holder.to_account_info().clone(),
                to: ctx.accounts.ori_refund_receiver.to_account_info().clone(),
                authority: ctx.accounts.currency_holder_auth.clone(),
            };
            let cpi_program = ctx.accounts.token_program.clone();
            let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
            token::transfer(cpi_ctx, auction.price)?;
        }

        // transfer bid pirce to custodial currency holder
        let cpi_accounts = Transfer {
            from: ctx.accounts.from.to_account_info().clone(),
            to: ctx.accounts.currency_holder.to_account_info().clone(),
            authority: ctx.accounts.from_auth.clone(),
        };
        let cpi_program = ctx.accounts.token_program.clone();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, price)?;

        // update auction info
        let auction = &mut ctx.accounts.auction;
        auction.bidder = *ctx.accounts.bidder.key;
        auction.refund_receiver = *ctx.accounts.from.to_account_info().key;
        auction.price = price;

        let bid = &mut ctx.accounts.bid;
        bid.price = price;
        bid.bidder = *ctx.accounts.bidder.key;

        Ok(())
    }

    pub fn close_auction(ctx: Context<CloseAuction>) -> Result<()> {
        let auction = &mut ctx.accounts.auction;

        let (_, seed) =
            Pubkey::find_program_address(&[&auction.seller.to_bytes()], &ctx.program_id);
        let seeds = &[auction.seller.as_ref(), &[seed]];
        let signer = &[&seeds[..]];

        // item ownership transfer
        let cpi_accounts = Transfer {
            from: ctx.accounts.item_holder.to_account_info().clone(),
            to: ctx.accounts.item_receiver.to_account_info().clone(),
            authority: ctx.accounts.item_holder_auth.clone(),
        };
        let cpi_program = ctx.accounts.token_program.clone();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, ctx.accounts.item_holder.amount)?;

        // currency ownership transfer
        if ctx.accounts.currency_holder.amount >= auction.price {
            let cpi_accounts = Transfer {
                from: ctx.accounts.currency_holder.to_account_info().clone(),
                to: ctx.accounts.currency_receiver.to_account_info().clone(),
                authority: ctx.accounts.currency_holder_auth.clone(),
            };
            let cpi_program = ctx.accounts.token_program.clone();
            let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
            token::transfer(cpi_ctx, auction.price)?;
        }

        auction.ongoing = false;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct CreateAuction<'info> {
    #[account(
        init,
        payer = seller,
    )]
    auction: Box<Account<'info, Auction>>,
    #[account(mut)]
    seller: Signer<'info>,
    #[account(
        constraint = item_holder.owner == auction_singer.key()
    )]
    item_holder: CpiAccount<'info, TokenAccount>,
    #[account(
        constraint = currency_holder.owner == auction_singer.key()
    )]
    currency_holder: CpiAccount<'info, TokenAccount>,
    /// CHECK: This is auction signer. no need to check
    auction_singer: UncheckedAccount<'info>,
    rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct Bid<'info> {
    #[account(
        mut, 
        constraint = auction.ongoing,
        has_one = currency_holder
    )]
    auction: Box<Account<'info, Auction>>,
    #[account(
        init,
        payer = bidder,
    )]
    bid: Box<Account<'info, Bid>>,
    bidder: Signer<'info>,
    #[account(
        mut,
        constraint = from.mint == currency_holder.mint,
        constraint = &from.owner == bidder.key
    )]
    from: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
    )]
    currency_holder: Box<Account<'info, TokenAccount>>,
    #[account(
        constraint = &currency_holder.owner == auction_singer.key
    )]
    /// CHECK: This is auction signer. no need to check
    auction_singer: UncheckedAccount<'info>,
    #[account(
        mut, 
        constraint = ori_refund_receiver.key == &Pubkey::default() || ori_refund_receiver.key == &auction.refund_receiver
    )]
    ori_refund_receiver: Box<Account<'info, TokenAccount>>,
    token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct CloseAuction<'info> {
    #[account(
        mut, 
        constraint = auction.ongoing
    )]
    auction: Box<Account<'info, Auction>>,
    seller: Signer<'info>,
    #[account(
        mut,
        constraint = item_holder.to_account_info().key == &auction.item_holder,
        constraint = &item_holder.owner == auction_singer.key(
    )]
    item_holder: CpiAccount<'info, TokenAccount>,
    /// CHECK: This is auction signer. no need to check
    auction_singer: UncheckedAccount<'info>,
    #[account(
        mut, 
        constraint = item_receiver.owner == auction.bidder
    )]
    item_receiver: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        constraint = currency_holder.to_account_info().key == &auction.currency_holder,
        constraint = &currency_holder.owner == auction_singer.key(
    )]
    currency_holder: Box<Account<'info, TokenAccount>>,
    #[account(mut)]
    currency_receiver: Box<Account<'info, TokenAccount>>,
    token_program: Program<'info, Token>,
}

#[account]
pub struct Auction {
    ongoing: bool,
    seller: Pubkey,
    item_holder: Pubkey,
    currency_holder: Pubkey,
    bidder: Pubkey,
    refund_receiver: Pubkey,
    price: u64,
}

#[account]
pub struct Bid {
    bidder: Pubkey,
    price: u64,
}

#[error]
pub enum AuctionErr {
    #[msg("your bid price is too low")]
    BidPirceTooLow,
}
