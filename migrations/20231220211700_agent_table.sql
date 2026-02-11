-- Create agent table
CREATE TABLE agent (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    url VARCHAR(255) NOT NULL UNIQUE,
    username VARCHAR(255) NOT NULL,
    password VARCHAR(255) NOT NULL,
    active BOOLEAN NOT NULL DEFAULT 1
);

-- Create index on url for faster lookups
CREATE INDEX idx_agent_url ON agent(url);
