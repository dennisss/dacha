
Creates these files under `/tmp`:

```
-rw-rw-r-- 1 dennis dennis     246 Sep  6 11:17 main.dennis-pc.dennis.log.ERROR.20210906-111708.3030672
-rw-rw-r-- 1 dennis dennis     320 Sep  6 11:17 main.dennis-pc.dennis.log.INFO.20210906-111708.3030672
-rw-rw-r-- 1 dennis dennis     246 Sep  6 11:17 main.dennis-pc.dennis.log.WARNING.20210906-111708.3030672
lrwxrwxrwx 1 dennis dennis      55 Sep  6 11:17 main.ERROR -> main.dennis-pc.dennis.log.ERROR.20210906-111708.3030672
lrwxrwxrwx 1 dennis dennis      54 Sep  6 11:17 main.INFO -> main.dennis-pc.dennis.log.INFO.20210906-111708.3030672
lrwxrwxrwx 1 dennis dennis      57 Sep  6 11:17 main.WARNING -> main.dennis-pc.dennis.log.WARNING.20210906-111708.3030672
```

Info Log:

```
Log file created at: 2021/09/06 11:17:08
Running on machine: dennis-pc
Running duration (h:mm:ss): 0:00:00
Log line format: [IWEF]yyyymmdd hh:mm:ss.uuuuuu threadid file:line] msg
I20210906 11:17:08.921416 3030672 main.cc:7] Testing logging of15 cookies
E20210906 11:17:08.921581 3030672 main.cc:8] And this is an error!
```

Error Log:

```
Log file created at: 2021/09/06 11:17:08
Running on machine: dennis-pc
Running duration (h:mm:ss): 0:00:00
Log line format: [IWEF]yyyymmdd hh:mm:ss.uuuuuu threadid file:line] msg
E20210906 11:17:08.921581 3030672 main.cc:8] And this is an error!
```


Note: Uses local time.