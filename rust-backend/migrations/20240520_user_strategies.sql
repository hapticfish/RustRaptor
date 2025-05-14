CREATE TABLE user_strategies (
                                 strategy_id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
                                 user_id     BIGINT NOT NULL,
                                 name        TEXT   NOT NULL,           -- 'mean_reversion'
                                 params      JSONB  NOT NULL,
                                 status      TEXT   NOT NULL DEFAULT 'enabled',
                                 created_at  TIMESTAMPTZ DEFAULT now()
);
