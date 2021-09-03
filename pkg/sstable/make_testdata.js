/*
This script creates golden SSTable test files with known contents using the reference
RocksDB/LevelDB implementations.

This script should be run without any arguments from the root directory of this repo:

rm -r testdata/sstable/
mkdir testdata/sstable/
node pkg/sstable/make_testdata.js
*/

var levelup = require('levelup');
var rocksdb = require('rocksdb');
var leveldown = require('leveldown');

// 1) Create our store


async function compactDb(db) {
  await new Promise((res, rej) => {
    db.compactRange('0', 'z', (e) => {
      if (e) {
        rej(e);
      } else {
        res();
      }
    });
  });
}

(async () => {

  {
    var db = levelup(leveldown('./testdata/sstable/leveldb-empty'));
    await db.close();
  }

  let food_dict = {
    'apples': 'fruit',
    'oranges': 'color',
    'mozzarella': 'cheese',
    'pizza': 'italy'
  };

  {
    var db = levelup(leveldown('./testdata/sstable/leveldb-food'));
    for (const key in food_dict) {
      await db.put(key, food_dict[key]);
    }

    await compactDb(db);

    await db.close();
  }

  {
    var db = levelup(leveldown('./testdata/sstable/leveldb-prefixed'));

    for (let i = 10000; i < 10100; i++) {
      await db.put(i + '', i % 2 == 0 ? 'even' : 'odd');
    }
    for (let i = 20000; i < 20100; i++) {
      await db.put(i + '', i % 2 == 0 ? 'even' : 'odd');
    }
    for (let i = 5000; i < 5100; i++) {
      await db.put(i + '', i % 2 == 0 ? 'even' : 'odd');
    }

    await compactDb(db);

    for (let i = 3200; i < 3300; i++) {
      await db.put(i + '', i % 2 == 0 ? 'even' : 'odd');
    }

    await db.close();
  }

  {
    var db = levelup(leveldown('./testdata/sstable/leveldb-food-mutate'));
    for (const key in food_dict) {
      await db.put(key, food_dict[key]);
    }

    await db.put('apples', 'cool');
    await db.del('mozzarella');

    await compactDb(db);

    await db.put('pizza', 'here');

    await db.close();
  }

  process.exit(0);

})().catch((e) => {
  console.log(e)
  process.exit(1);
});
