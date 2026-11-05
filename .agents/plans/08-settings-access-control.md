# Protect company settings

Settings are currently editable by anyone who can use the application. Before using settings changes in fiscal issuance, decide who may edit them and how the operator recovers access.

- Define the threat model, including local computer access, account sharing, support and recovery; distinguish application access controls from operating-system and database access.
- Choose an appropriate authentication and authorization mechanism, secure credential storage, change auditing, and recovery procedure. Never store plaintext passwords or production credentials in the repository.
- Test authorized and unauthorized changes, lockout and recovery. Check that company settings cannot silently change the identity on already authorized fiscal documents.

Keep this plan open until the mechanism and operator workflow are reviewed and tested.
