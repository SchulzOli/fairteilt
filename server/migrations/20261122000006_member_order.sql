-- Stable member order (join order), independent of clock jumps between transactions
-- (joined_at alone flipped the order when the database clock was corrected).
ALTER TABLE memberships ADD COLUMN seq BIGINT GENERATED ALWAYS AS IDENTITY;
