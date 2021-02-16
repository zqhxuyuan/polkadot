# Using BEEFY dependency (`git subtree`).

# How to fix broken BEEFY code?

To fix BEEFY code simply create a commit in this repo. Best if the commit is isolated to `beefy` sub-directory.

# How to pull latest BEEFY or contribute back?

1. Add BEEFY repo as a remote:
```
$ git remote add -f beefy git@github.com:paritytech/grandpa-bridge-gadget.git
```
If you plan to contribute back, consider forking and adding your personal fork as well.
```
$ git remote add -f my-beefy git@github.com:tomusdrw/grandpa-bridge-gadget.git
```

2. To update BEEFY:
```
$ git fetch beefy master
$ git subtree pull --prefix=beefy beefy master --squash
````

3. To contribute back to BEEFY.
```
$ git subtree push --prefix=beefy my-beefy master
```
And then simply create a PR to the main repo.

