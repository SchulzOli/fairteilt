-- Sign in with Google: Google's stable account id ("sub" of the ID token). An account found by
-- its verified e-mail address is linked on the first Google sign-in.
ALTER TABLE users ADD COLUMN google_sub TEXT UNIQUE;

-- An account with an e-mail address signs in with a password, with Google, or both. Child
-- profiles and deleted accounts have neither.
ALTER TABLE users DROP CONSTRAINT users_login_complete;
ALTER TABLE users ADD CONSTRAINT users_login_complete CHECK (
    (email IS NULL AND password_hash IS NULL AND google_sub IS NULL)
    OR (email IS NOT NULL AND (password_hash IS NOT NULL OR google_sub IS NOT NULL))
);
