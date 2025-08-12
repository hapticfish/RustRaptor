-- 20250601_add-strategy-columns.sql  (safe to re-run anywhere)
---------------------------------------------------------------
ALTER TABLE user_strategies
    ADD COLUMN IF NOT EXISTS exchange  TEXT NOT NULL DEFAULT 'blowfin',
    ADD COLUMN IF NOT EXISTS symbol    TEXT NOT NULL DEFAULT 'BTCUSDT',
    ADD COLUMN IF NOT EXISTS strategy  TEXT NOT NULL DEFAULT 'mean_reversion';

----------------------------------------------------------------
-- Move old data only if the legacy column still exists
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name   = 'user_strategies'
          AND column_name  = 'name'
    ) THEN
        EXECUTE $m$
UPDATE user_strategies
SET    strategy = name
WHERE  strategy = 'mean_reversion'
   OR  strategy IS NULL;
$m$;
END IF;
END$$;

-- Drop legacy column if it’s still around
ALTER TABLE user_strategies
DROP COLUMN IF EXISTS name;

----------------------------------------------------------------
-- Helpful indexes (skip if already present)
CREATE INDEX IF NOT EXISTS idx_user_strategies_user   ON user_strategies (user_id);
CREATE INDEX IF NOT EXISTS idx_user_strategies_status ON user_strategies (status);
CREATE INDEX IF NOT EXISTS idx_user_strategies_pair   ON user_strategies (exchange, symbol);
