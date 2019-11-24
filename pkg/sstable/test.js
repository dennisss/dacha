var levelup = require('levelup');
var rocksdb = require('rocksdb');
var leveldown = require('leveldown');

// 1) Create our store
var db = levelup(leveldown('./testdata/leveldb'));



(async () => {

    for (var i = 0; i < 10000; i++) {
        await db.put((new Date()).getTime()  + '-' + i, 'time' + i);
    }


})().catch((e) => {
    console.log(e)
});

/*
var ops = [
    { type: 'put', key: 'apples', value: 'fruit' },
    { type: 'put', key: 'oranges', value: 'color' },
    { type: 'put', key: 'mozzarella', value: 'cheese' },
    { type: 'put', key: 'pizza', value: 'italy' }
  ]

  db.batch(ops, function (err) {
    if (err) return console.log('Ooops!', err)
    console.log('Great success dear leader!')
  })
*/

/*
// 2) Put a key & value
db.put('name', 'levelup', function (err) {
  if (err) return console.log('Ooops!', err) // some kind of I/O error


})
*/