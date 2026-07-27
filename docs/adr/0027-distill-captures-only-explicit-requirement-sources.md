# Distill captures only explicit requirement sources

Intake snapshots only requirement text, files, links, or prior user messages explicitly selected for the run. A bare invocation asks the user for a requirement rather than scraping the existing conversation, while an explicit instruction such as “use the discussion above” authorizes the intake executor to select and record relevant user messages; system prompts, tool output, and assistant reasoning are never treated as original requirement sources.
