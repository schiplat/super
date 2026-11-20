-- Session rows gained two facts the login flow needs after the fact:
--   email_verified — role rules with an `email` selector require a verified
--                    address, and the decision is made per request, so the
--                    claim has to survive with the session.
--   return_to      — where to send the browser after the one-time exchange.

ALTER TABLE sessions ADD COLUMN email_verified INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sessions ADD COLUMN return_to TEXT NOT NULL DEFAULT '/';
