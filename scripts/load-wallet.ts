/**
 * Reads DEPLOY_PRIVATE_KEY (base58) from .env and writes the 64-byte keypair
 * JSON that Solana CLI / Anchor expect.
 *
 * Output: .anchor/deploy-wallet.json (gitignored)
 * Run:    npx -y tsx scripts/load-wallet.ts
 */

import { readFileSync, writeFileSync, mkdirSync, existsSync } from "node:fs";

const ALPHABET = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

function b58decode(s: string): Uint8Array {
    let num = 0n;
    for (const c of s) {
        const idx = ALPHABET.indexOf(c);
        if (idx < 0) throw new Error(`invalid base58 character: ${c}`);
        num = num * 58n + BigInt(idx);
    }
    const bytes: number[] = [];
    while (num > 0n) {
        bytes.unshift(Number(num & 0xffn));
        num >>= 8n;
    }
    const leadingOnes = s.length - s.replace(/^1+/, "").length;
    return new Uint8Array([...Array(leadingOnes).fill(0), ...bytes]);
}

function main() {
    if (!existsSync(".env")) {
        console.error(
            "missing .env — create it from .env.example and paste your Phantom base58 key",
        );
        process.exit(1);
    }
    const env = readFileSync(".env", "utf8");
    const match = env.match(/^DEPLOY_PRIVATE_KEY=(.+)$/m);
    if (!match) {
        console.error("DEPLOY_PRIVATE_KEY not found in .env");
        process.exit(1);
    }
    const key = match[1].trim().replace(/^['"]|['"]$/g, "");
    if (!key) {
        console.error("DEPLOY_PRIVATE_KEY is empty in .env");
        process.exit(1);
    }

    const data = b58decode(key);
    if (data.length !== 64) {
        console.error(
            `expected 64 bytes, got ${data.length} — Phantom export should decode to 64 bytes`,
        );
        process.exit(1);
    }

    mkdirSync(".anchor", { recursive: true });
    const outPath = ".anchor/deploy-wallet.json";
    writeFileSync(outPath, JSON.stringify(Array.from(data)));
    console.log(`wrote ${outPath}`);
    console.log("\nnext:");
    console.log(
        "  solana config set --keypair .anchor/deploy-wallet.json --url devnet",
    );
    console.log("  solana address    # should match your Phantom address");
    console.log("  solana balance");
}

main();
