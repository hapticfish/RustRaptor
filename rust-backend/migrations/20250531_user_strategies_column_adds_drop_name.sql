-- 2024-06-01-add-strategy-columns.sql
------------------------------------------------------------
ALTER TABLE user_strategies
    ADD COLUMN exchange  TEXT    NOT NULL DEFAULT 'blowfin',
    ADD COLUMN symbol    TEXT    NOT NULL DEFAULT 'BTCUSDT',
    ADD COLUMN strategy  TEXT    NOT NULL DEFAULT 'mean_reversion';

-- back-fill the new `strategy` column with the old `name`
UPDATE user_strategies
SET    strategy = name
WHERE  strategy = 'mean_reversion'          -- only rows inserted before
   OR  strategy IS NULL;

-- (optional) drop the legacy `name` column once you’re happy
ALTER TABLE user_strategies
DROP COLUMN name;
------------------------------------------------------------
-- indices you’ll probably want later
CREATE INDEX idx_user_strategies_user   ON user_strategies (user_id);
CREATE INDEX idx_user_strategies_status ON user_strategies (status);
CREATE INDEX idx_user_strategies_pair   ON user_strategies (exchange, symbol);