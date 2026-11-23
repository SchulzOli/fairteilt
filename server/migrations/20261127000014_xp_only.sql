-- Only XP since 23.11.2026: no Guthaben (G), no transfers, no rewards catalog.

-- Settlement rules that paid or took G: the difference carries over into the next period instead.
UPDATE households
   SET period_policy = jsonb_set(period_policy, '{deficit_action}', '{"type": "carry_over", "percent": 100}')
 WHERE period_policy->'deficit_action'->>'type' IN ('balance_deduction', 'zero_sum');
UPDATE households
   SET period_policy = jsonb_set(period_policy, '{surplus_action}', '{"type": "carry_over", "percent": 100}')
 WHERE period_policy->'surplus_action'->>'type' = 'bonus';

-- The G switches of the point rules are gone (bonus XP has its own monthly allowance).
UPDATE households
   SET point_policy = point_policy - 'bounty_enabled' - 'bounty_max_per_pledge' - 'transfer_enabled'
                      - 'transfer_max_per_period' - 'child_transfer_needs_approval' - 'allow_negative_balance';

-- Transfers and the rewards catalog. Their old ledger entries stay in the books (ref_id only).
DROP TABLE reward_redemptions;
DROP TABLE rewards;
DROP TABLE transfers;
