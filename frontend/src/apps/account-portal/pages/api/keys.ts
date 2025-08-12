import type { NextApiRequest, NextApiResponse } from 'next'
import { seal } from '@/lib/envelopeCrypto';      // thin TS wrapper around your Rust WASM or JS lib
import * as z from 'zod';

export default async function handler(req:NextApiRequest,res:NextApiResponse) {
    if (req.method !== 'POST') return res.status(405).end();

    const bodySchema = z.object({
        apiKey: z.string().min(10),
        secret: z.string().min(10),
        pass:   z.string().optional(),
    });
    const { apiKey, secret, pass } = bodySchema.parse(req.body);

    // 1. Encrypt with the PUBLIC key from env
    const sealed = seal({ apiKey, secret, passphrase: pass }, process.env.MASTER_PK_PEM!);

    // 2. Forward to Rust back-end
    await fetch(`${process.env.BACKEND_URL}/api_keys`, {
        method:'POST',
        headers:{'Content-Type':'application/json', 'Authorization':`Bearer ${req.cookies.jwt}`},
        body: JSON.stringify(sealed),
    });

    res.status(200).json({ ok:true });
}
