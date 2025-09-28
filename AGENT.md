# AGENT.md

# Explicit wishes of the user to follow on each turn

## After having commited your changes always do the following step by step:

1) Actual date

Remember the current date, Make sure to insert these informations wherever appropriate. If unsure use the shell command `date` to fetch the current date.

2) CHANGES.md entry

Insert an entry at the top of the file CHANGES.md and describe the implemented changes. The entry begins with the date (YYYY-MM-DD) and the author (if known)
with his email. Use the current date and the default users's email address for these. Next follows an empty line, followed by a short concise summary not
longer than 80 characters. Then follows an empty line and below that the list of changes gets described.

3) Score the source code

If the source code has been changed, move the file SCORE.md to SCORE-before.md, before overwriting it (do not use -f with mv).
Score the source code and write a comprehensive scoring, inclusive grades and grade summary and proposed next steps to the file SCORE.md in Markdown.

4) Get user confirmation to proceed

Ask the user: "Shall I proceed with these next steps?".
