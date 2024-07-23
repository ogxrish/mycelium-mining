import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Test } from "../target/types/test";
import { PublicKey } from "@solana/web3.js";
import { createMint, createAssociatedTokenAccount, mintTo, getAssociatedTokenAddressSync } from "@solana/spl-token";
describe("test", () => {
  // Configure the client to use the local cluster.
  const provider = anchor.AnchorProvider.env();
  const wallet = provider.wallet as anchor.Wallet;
  anchor.setProvider(provider);
  const program = anchor.workspace.Test as Program<Test>;

  let mint: PublicKey = new PublicKey("J43bGRufM646mhHgibfzdxPAZy6jLxx6ccK1ACuokqcT");
  const initializeMint = async () => {
    if (mint) return;
    mint = await createMint(
      provider.connection,
      wallet.payer,
      wallet.publicKey,
      null,
      9,
    );
    const tokenAccount = await createAssociatedTokenAccount(
      provider.connection,
      wallet.payer,
      mint,
      wallet.publicKey,
    );
    await mintTo(
      provider.connection,
      wallet.payer,
      mint,
      tokenAccount,
      wallet.payer,
      100_000 * 10 ** 9
    )
    const tokenAccount2 = await createAssociatedTokenAccount(
      provider.connection,
      wallet.payer,
      mint,
      new PublicKey("58V6myLoy5EVJA3U2wPdRDMUXpkwg8Vfw5b6fHqi2mEj")
    );
    await mintTo(
      provider.connection,
      wallet.payer,
      mint,
      tokenAccount2,
      wallet.payer,
      100000 * 10 ** 9
    )
  }
  it("initializes and starts mining", async () => {
    // Add your test here.
    await initializeMint();
    console.log(mint.toString());

    const i1 = await program.methods.initialize().accounts({
      signer: wallet.publicKey,
      mint,
    }).instruction();
    const i2 = await program.methods.newEpoch(new anchor.BN(1)).accounts({
      signer: wallet.publicKey
    }).instruction();
    const tx = new anchor.web3.Transaction();
    tx.add(i1, i2);
    await provider.sendAndConfirm(tx);
  });
  it("mines", async () => {
    await program.methods.mine(new anchor.BN(1)).accounts({
      signer: wallet.publicKey
    }).rpc();

  });
  it("funds", async () => {
    const signerTokenAccount = getAssociatedTokenAddressSync(mint, wallet.publicKey);
    await program.methods.fundProgramToken(new anchor.BN(50000)).accounts({
      signer: wallet.publicKey,
      signerTokenAccount,
    }).rpc();
  });
  it("withdraws", async () => {
    await program.methods.withdrawFees(new anchor.BN(100)).accounts({
      signer: wallet.publicKey,
    }).rpc();
  })
  it("claims", async () => {
    await new Promise((resolve) => setTimeout(resolve, 10000));
    await program.methods.newEpoch(new anchor.BN(2)).accounts({
      signer: wallet.publicKey
    }).rpc();
    const signerTokenAccount = getAssociatedTokenAddressSync(mint, wallet.publicKey);
    await program.methods.claim(new anchor.BN(1)).accounts({
      signer: wallet.publicKey,
      signerTokenAccount,
    }).rpc();
  })
});
