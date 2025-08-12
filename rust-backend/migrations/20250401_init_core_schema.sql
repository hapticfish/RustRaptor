-- migrations/20240503_init_core_schema.sql
-- Enable UUID generator
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

--------------------------------------------------------------------
--  ENUMs  (keeps columns compact & validated)
--------------------------------------------------------------------
CREATE TYPE order_status     AS ENUM ('live','partially_filled','filled','cancelled','rejected');
CREATE TYPE order_type_enum  AS ENUM ('market','limit','post_only','fok','ioc','trigger','conditional');
CREATE TYPE maker_taker_enum AS ENUM ('maker','taker');
CREATE TYPE fee_type_enum    AS ENUM ('maker','taker','funding','rebate');
CREATE TYPE market_type_enum AS ENUM ('spot','futures','swap','options');

--------------------------------------------------------------------
-- 1. users  --------------------------------------------------------
--------------------------------------------------------------------
CREATE TABLE users (
                       user_id        BIGINT PRIMARY KEY,          -- Discord snowflake
                       rr_username    VARCHAR(32) UNIQUE NOT NULL,
                       email          VARCHAR(255),
                       created_at     TIMESTAMPTZ DEFAULT now()
);

--------------------------------------------------------------------
-- 2. api_keys  (encrypted before insert) ---------------------------
--------------------------------------------------------------------
CREATE TABLE api_keys (
                          key_id                UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
                          user_id               BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
                          exchange              VARCHAR(16) NOT NULL, -- 'blowfin', 'binance', ...
                          encrypted_api_key     BYTEA NOT NULL,
                          encrypted_secret      BYTEA NOT NULL,
                          encrypted_passphrase  BYTEA,
                          created_at            TIMESTAMPTZ DEFAULT now(),
                          UNIQUE (user_id, exchange)  -- max 1 active key / exch / user
);

--------------------------------------------------------------------
-- 3. exchange_accts  ----------------------------------------------
--------------------------------------------------------------------
CREATE TABLE exchange_accts (
                                acct_id         UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
                                user_id         BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
                                exchange        VARCHAR(16) NOT NULL,
                                label           VARCHAR(64) DEFAULT 'default',
                                leverage_default NUMERIC(6,2),
                                demo            BOOLEAN DEFAULT false,
                                CONSTRAINT uniq_acct UNIQUE (user_id, exchange, label)
);

--------------------------------------------------------------------
-- 4. strategies  ---------------------------------------------------
--------------------------------------------------------------------
CREATE TABLE strategies (
                            strategy_id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
                            user_id     BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
                            name        VARCHAR(48) NOT NULL,
                            params      JSONB        NOT NULL,
                            active      BOOLEAN      DEFAULT true,
                            updated_at  TIMESTAMPTZ  DEFAULT now()
);

--------------------------------------------------------------------
-- 5. copy_relations  ----------------------------------------------
--------------------------------------------------------------------
CREATE TABLE copy_relations (
                                relation_id       UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
                                leader_user_id    BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
                                follower_user_id  BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
                                since             TIMESTAMPTZ DEFAULT now(),
                                until             TIMESTAMPTZ,
                                status            VARCHAR(16) DEFAULT 'active',
                                CONSTRAINT no_self_copy CHECK (leader_user_id <> follower_user_id),
                                UNIQUE (leader_user_id, follower_user_id, since)
);

--------------------------------------------------------------------
-- 6. orders  -------------------------------------------------------
--------------------------------------------------------------------
CREATE TABLE orders (
                        order_id           UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
                        external_order_id  VARCHAR(64),
                        user_id            BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
                        exchange           VARCHAR(16) NOT NULL,
                        market_type        market_type_enum NOT NULL,      -- spot / futures / swap …
                        symbol             VARCHAR(32) NOT NULL,           -- BTC‑USDT etc.
                        side               VARCHAR(4)  NOT NULL,           -- buy / sell
                        order_type         order_type_enum NOT NULL,
                        price              NUMERIC,
                        size               NUMERIC NOT NULL,
                        reduce_only        BOOLEAN DEFAULT false,
                        margin_mode        VARCHAR(8),                     -- cross / isolated
                        position_side      VARCHAR(5),                     -- net / long / short
                        status             order_status NOT NULL,
                        opened_at          TIMESTAMPTZ DEFAULT now(),
                        closed_at          TIMESTAMPTZ
);

CREATE INDEX orders_user_idx          ON orders(user_id);
CREATE INDEX orders_exchange_status   ON orders(exchange,status);
CREATE UNIQUE INDEX orders_extid_idx  ON orders(exchange,external_order_id);

--------------------------------------------------------------------
-- 7. fills  --------------------------------------------------------
--------------------------------------------------------------------
CREATE TABLE fills (
                       fill_id       UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
                       order_id      UUID NOT NULL REFERENCES orders(order_id) ON DELETE CASCADE,
                       maker_taker   maker_taker_enum NOT NULL,
                       fill_price    NUMERIC NOT NULL,
                       fill_size     NUMERIC NOT NULL,
                       trade_fee     NUMERIC DEFAULT 0,
                       funding_fee   NUMERIC DEFAULT 0,
                       realised_pnl  NUMERIC DEFAULT 0,          -- positive/negative
                       executed_at   TIMESTAMPTZ NOT NULL
);

CREATE INDEX fills_order_idx ON fills(order_id);

--------------------------------------------------------------------
-- 8. fees  (stand‑alone events) -----------------------------------
--------------------------------------------------------------------
CREATE TABLE fees (
                      fee_id        UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
                      user_id       BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
                      exchange      VARCHAR(16) NOT NULL,
                      symbol        VARCHAR(32),
                      fee_type      fee_type_enum NOT NULL,
                      amount        NUMERIC NOT NULL,
                      reference_id  UUID,                    -- could point to order / fill
                      occurred_at   TIMESTAMPTZ NOT NULL
);

--------------------------------------------------------------------
-- 9. positions snapshot -------------------------------------------
--------------------------------------------------------------------
CREATE TABLE positions (
                           snapshot_id       UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
                           user_id           BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
                           exchange          VARCHAR(16) NOT NULL,
                           symbol            VARCHAR(32) NOT NULL,
                           market_type       market_type_enum NOT NULL,
                           side              VARCHAR(5) NOT NULL,         -- long / short / net
                           size              NUMERIC NOT NULL,
                           avg_entry_price   NUMERIC,
                           unrealised_pnl    NUMERIC,
                           leverage          NUMERIC(6,2),
                           liquidation_price NUMERIC,
                           captured_at       TIMESTAMPTZ NOT NULL
);

CREATE INDEX pos_user_symbol_time ON positions(user_id,symbol,captured_at DESC);

--------------------------------------------------------------------
-- 10. balances snapshot -------------------------------------------
--------------------------------------------------------------------
CREATE TABLE balances (
                          snapshot_id      UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
                          user_id          BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
                          exchange         VARCHAR(16) NOT NULL,
                          currency         VARCHAR(16) NOT NULL,
                          equity           NUMERIC,
                          available        NUMERIC,
                          isolated_equity  NUMERIC,
                          captured_at      TIMESTAMPTZ NOT NULL
);

--------------------------------------------------------------------
-- 11. copy_events  -------------------------------------------------
--------------------------------------------------------------------
CREATE TABLE copy_events (
                             copy_id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
                             leader_order_id      UUID NOT NULL REFERENCES orders(order_id) ON DELETE CASCADE,
                             follower_order_id    UUID NOT NULL REFERENCES orders(order_id) ON DELETE CASCADE,
                             slippage_bps         NUMERIC(10,4),
                             copied_at            TIMESTAMPTZ DEFAULT now()
);

--------------------------------------------------------------------
-- 12. audit_log  ---------------------------------------------------
--------------------------------------------------------------------
CREATE TABLE audit_log (
                           event_id    UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
                           user_id     BIGINT REFERENCES users(user_id),
                           action      VARCHAR(64) NOT NULL,
                           details     JSONB,
                           ts          TIMESTAMPTZ DEFAULT now()
);
