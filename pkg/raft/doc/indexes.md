

Match Index
-----------
- Gurantees for this are the loosest of the bunch as things may trivially get truncated in some scenarios so things are not necessarily monotonic


Commit Position
---------------
- Commit index is monotonic
	- We will never see a commit in the future with a lower term
- Commit term is monotonic
	- We will never see a commit with a lower term in the future
- Therefore if commit_index >= entry_index || commit_term > entry_term, then:
	- 'entry' is definately either commited or will never be commited

Applied Position
----------------
Meaning the index/term of the last entry applied to the state machine
- Naturally follows the commit_index/commit_term
- If the commit position is beyond a single entry's position, then we may be able to determine that the entry may never be applied based on the current information in the log
