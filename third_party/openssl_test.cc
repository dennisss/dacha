// g++ third_party/openssl_test.cc -lcrypto

#include <cstdio>
#include <openssl/des.h>

int main(int argc, char** argv) {

    DES_key_schedule ks;

    DES_cblock key = {
        0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
    };

    DES_set_key((const_DES_cblock*) key, &ks);

    for (int i = 0; i < 16; i++) {
        printf("[");
        for(int j = 0; j < 8; j++) {
            printf("0x%x, ", ks.ks[i].cblock[j]);
        }
        printf("],\n");
    }

    return 0;
}