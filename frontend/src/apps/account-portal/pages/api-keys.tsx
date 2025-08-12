import React, { useState } from 'react';

export default function ApiKeysPage() {
    const [apiKey, setKey] = useState('');
    const [secret,  setSecret] = useState('');
    const [pass,    setPass] = useState('');

    async function handleSubmit(e: React.FormEvent) {
        e.preventDefault();
        const res = await fetch('/api/keys', {          // ← calls API Route below
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ apiKey, secret, pass }),
        });
        if (res.ok) alert('Keys stored!');
        else        alert('Failed – check console');
    }

    return (
        <form onSubmit={handleSubmit} className="space-y-4">
            <input value={apiKey} onChange={e=>setKey(e.target.value)} placeholder="API key"  />
            <input value={secret}  onChange={e=>setSecret(e.target.value)} placeholder="Secret"  />
            <input value={pass}    onChange={e=>setPass(e.target.value)} placeholder="Passphrase" />
            <button type="submit">Save</button>
        </form>
    );
}
