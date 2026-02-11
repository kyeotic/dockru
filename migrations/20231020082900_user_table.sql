-- Create user table
CREATE TABLE user (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    username VARCHAR(255) NOT NULL UNIQUE COLLATE NOCASE,
    password VARCHAR(255),
    active BOOLEAN NOT NULL DEFAULT 1,
    timezone VARCHAR(150),
    twofa_secret VARCHAR(64),
    twofa_status BOOLEAN NOT NULL DEFAULT 0,
    twofa_last_token VARCHAR(6)
);

-- Create index on username for faster lookups
CREATE INDEX idx_user_username ON user(username);
