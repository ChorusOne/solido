/**
 * Flow Library definition for spl-token-swap
 *
 * This file is manually maintained
 *
 */

declare module '@solana/spl-token-swap' {
  // === client/token-swap.js ===
  declare export class Numberu64 extends BN {
    toBuffer(): Buffer;
    static fromBuffer(buffer: Buffer): Numberu64;
  }

  declare export var TokenSwapLayout: Layout;

  declare export var CurveType: Object;

  declare export class TokenSwap {
    constructor(
      connection: Connection,
      tokenSwap: PublicKey,
      swapProgramId: PublicKey,
      tokenProgramId: PublicKey,
      poolToken: PublicKey,
      feeAccount: PublicKey,
      authority: PublicKey,
      tokenAccountA: PublicKey,
      tokenAccountB: PublicKey,
      mintA: PublicKey,
      mintB: PublicKey,
      tradeFeeNumerator: Numberu64,
      tradeFeeDenominator: Numberu64,
      ownerTradeFeeNumerator: Numberu64,
      ownerTradeFeeDenominator: Numberu64,
      ownerWithdrawFeeNumerator: Numberu64,
      ownerWithdrawFeeDenominator: Numberu64,
      hostFeeNumerator: Numberu64,
      hostFeeDenominator: Numberu64,
      curveType: number,
      payer: Account,
    ): TokenSwap;

    static getMinBalanceRentForExemptTokenSwap(
      connection: Connection,
    ): Promise<number>;

    static createInitSwapInstruction(
      tokenSwapAccount: Account,
      authority: PublicKey,
      tokenAccountA: PublicKey,
      tokenAccountB: PublicKey,
      tokenPool: PublicKey,
      feeAccount: PublicKey,
      tokenAccountPool: PublicKey,
      tokenProgramId: PublicKey,
      swapProgramId: PublicKey,
      nonce: number,
      tradeFeeNumerator: number,
      tradeFeeDenominator: number,
      ownerTradeFeeNumerator: number,
      ownerTradeFeeDenominator: number,
      ownerWithdrawFeeNumerator: number,
      ownerWithdrawFeeDenominator: number,
      hostFeeNumerator: number,
      hostFeeDenominator: number,
      curveType: number,
    ): TransactionInstruction;

    static loadTokenSwap(
      connection: Connection,
      address: PublicKey,
      programId: PublicKey,
      payer: Account,
    ): Promise<TokenSwap>;

    static createTokenSwap(
      connection: Connection,
      payer: Account,
      tokenSwapAccount: Account,
      authority: PublicKey,
      tokenAccountA: PublicKey,
      tokenAccountB: PublicKey,
      poolToken: PublicKey,
      mintA: PublicKey,
      mintB: PublicKey,
      feeAccount: PublicKey,
      tokenAccountPool: PublicKey,
      swapProgramId: PublicKey,
      tokenProgramId: PublicKey,
      nonce: number,
      tradeFeeNumerator: number,
      tradeFeeDenominator: number,
      ownerTradeFeeNumerator: number,
      ownerTradeFeeDenominator: number,
      ownerWithdrawFeeNumerator: number,
      ownerWithdrawFeeDenominator: number,
      hostFeeNumerator: number,
      hostFeeDenominator: number,
      curveType: number,
    ): Promise<TokenSwap>;

    swap(
      userSource: PublicKey,
      poolSource: PublicKey,
      poolDestination: PublicKey,
      userDestination: PublicKey,
      hostFeeAccount: ?PublicKey,
      userTransferAuthority: Account,
      amountIn: number | Numberu64,
      minimumAmountOut: number | Numberu64,
    ): Promise<TransactionSignature>;

    static swapInstruction(
      tokenSwap: PublicKey,
      authority: PublicKey,
      userTransferAuthority: PublicKey,
      userSource: PublicKey,
      poolSource: PublicKey,
      poolDestination: PublicKey,
      userDestination: PublicKey,
      poolMint: PublicKey,
      feeAccount: PublicKey,
      hostFeeAccount: ?PublicKey,
      swapProgramId: PublicKey,
      tokenProgramId: PublicKey,
      amountIn: number | Numberu64,
      minimumAmountOut: number | Numberu64,
    ): TransactionInstruction;

    depositAllTokenTypes(
      userAccountA: PublicKey,
      userAccountB: PublicKey,
      poolAccount: PublicKey,
      userTransferAuthority: Account,
      poolTokenAmount: number | Numberu64,
      maximumTokenA: number | Numberu64,
      maximumTokenB: number | Numberu64,
    ): Promise<TransactionSignature>;

    static depositAllTokenTypesInstruction(
      tokenSwap: PublicKey,
      authority: PublicKey,
      userTransferAuthority: PublicKey,
      sourceA: PublicKey,
      sourceB: PublicKey,
      intoA: PublicKey,
      intoB: PublicKey,
      poolToken: PublicKey,
      poolAccount: PublicKey,
      swapProgramId: PublicKey,
      tokenProgramId: PublicKey,
      poolTokenAmount: number | Numberu64,
      maximumTokenA: number | Numberu64,
      maximumTokenB: number | Numberu64,
    ): TransactionInstruction;

    withdrawAllTokenTypes(
      userAccountA: PublicKey,
      userAccountB: PublicKey,
      poolAccount: PublicKey,
      userTransferAuthority: Account,
      poolTokenAmount: number | Numberu64,
      minimumTokenA: number | Numberu64,
      minimumTokenB: number | Numberu64,
    ): Promise<TransactionSignature>;

    static withdrawAllTokenTypesInstruction(
      tokenSwap: PublicKey,
      authority: PublicKey,
      userTransferAuthority: PublicKey,
      poolMint: PublicKey,
      feeAccount: PublicKey,
      sourcePoolAccount: PublicKey,
      fromA: PublicKey,
      fromB: PublicKey,
      userAccountA: PublicKey,
      userAccountB: PublicKey,
      swapProgramId: PublicKey,
      tokenProgramId: PublicKey,
      poolTokenAmount: number | Numberu64,
      minimumTokenA: number | Numberu64,
      minimumTokenB: number | Numberu64,
    ): TransactionInstruction;

    depositSingleTokenTypeExactAmountIn(
      userAccount: PublicKey,
      poolAccount: PublicKey,
      userTransferAuthority: Account,
      sourceTokenAmount: number | Numberu64,
      minimumPoolTokenAmount: number | Numberu64,
    ): Promise<TransactionSignature>;

    static depositSingleTokenTypeExactAmountInInstruction(
      tokenSwap: PublicKey,
      authority: PublicKey,
      userTransferAuthority: PublicKey,
      source: PublicKey,
      intoA: PublicKey,
      intoB: PublicKey,
      poolToken: PublicKey,
      poolAccount: PublicKey,
      swapProgramId: PublicKey,
      tokenProgramId: PublicKey,
      sourceTokenAmount: number | Numberu64,
      minimumPoolTokenAmount: number | Numberu64,
    ): TransactionInstruction;

    withdrawSingleTokenTypeExactAmountOut(
      userAccount: PublicKey,
      poolAccount: PublicKey,
      userTransferAuthority: Account,
      destinationTokenAmount: number | Numberu64,
      maximumPoolTokenAmount: number | Numberu64,
    ): Promise<TransactionSignature>;

    static withdrawSingleTokenTypeExactAmountOutInstruction(
      tokenSwap: PublicKey,
      authority: PublicKey,
      userTransferAuthority: PublicKey,
      poolMint: PublicKey,
      feeAccount: PublicKey,
      sourcePoolAccount: PublicKey,
      fromA: PublicKey,
      fromB: PublicKey,
      userAccount: PublicKey,
      swapProgramId: PublicKey,
      tokenProgramId: PublicKey,
      destinationTokenAmount: number | Numberu64,
      maximumPoolTokenAmount: number | Numberu64,
    ): TransactionInstruction;
  }
}
