
var input = `...`;


input.trim().split('\n').map((line) => {
    let fields = line.split(' ');

    console.log(`pub const RADIO_${fields[0]}: *mut u32 = (RADIO + ${fields[1]}) as *mut u32;`);
})

