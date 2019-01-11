Store Data Directory Structure
------------------------------

Assuming the store data folder is set to `/hay`, the following files will be created:

- `/hay/lock`: Empty file for ensuring that only one store server instance is running on the current machine
- `/hay/volumes`: List of all volume ids in this directory
- `/hay/haystack_<logical_volume_id>`: Data for the local physical volume in this logical volume
- `/hay/haystack_<logical_volume_id>.idx`: Index of all needles in the corresponding volume

During the compaction of a volume, the following other two files will be present:
- `/hay/haystack_<logical_volume_id>.pack`
- `/hay/haystack_<logical_volume_id>.pack.idx`
- If the corresponding non-pack files are not present, then it can be assumed that the compaction was previously completed but the names of the new compacted files were not yet renamed to the regular names 