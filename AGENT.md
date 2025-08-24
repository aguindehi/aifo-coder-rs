# AGENT.md

# Explicit wishes of the user

## After having commited your changes always do the following step by step:

1. Remember the current date, time and default user. Make sure to insert these informations wherever appropriate. If unsure use the shell command `date` to fetch the current date and time.
2. Insert an entry at the top of the file CHANGES.md and describe the implemented changes. The entry begins with the date (YYYY-MM-DD), the time (HH:MM) and the author with email. Use the current date and time and the default users's email address for these. Next follows an empty line, followed by a short concise summary not longer than 80 characters. Then follows an empty line and below that the list of changes gets described.
3. If the source code has been changed, Move the file SCORE.md to SCORE-before.md, overwriting it. Do not use -f with mv, it overwrites anyway.
4. If the source code has been changed, score the source code and write a comprehensive scoring, inclusive grades and grade summary, to the file SCORE.md in Markdown.
5. Consider the next steps according to the notes in the fresh SCORE.md.
6. Propose next steps to the user.
7. Ask the user: "Shall I proceed with these next steps?".
