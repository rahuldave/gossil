#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <limits.h>
static void *fossil_malloc(size_t n){ void *p=malloc(n); if(!p) abort(); return p; }
#include "upstream-isspace.c"
#include "upstream-bodies.c"
static void hex(const unsigned char *p,size_t n){ for(size_t i=0;i<n;i++) printf("%02x",p[i]); }
int main(int argc,char **argv){
 if(argc!=4) return 2;
 size_t n=strlen(argv[2])/2; if(strlen(argv[2])%2 || n>4096) return 3;
 unsigned char *b=malloc(n+1); if(!b) return 4;
 for(size_t i=0;i<n;i++){unsigned x; if(sscanf(argv[2]+i*2,"%2x",&x)!=1) return 5; b[i]=(unsigned char)x;} b[n]=0;
 if(!strcmp(argv[1],"decode")){defossilize((char*)b); hex(b,n+1); printf(" %zu\n",strlen((char*)b));}
 else if(!strcmp(argv[1],"encode")) {int len=atoi(argv[3]); if(len < -1 || (len>=0 && (size_t)len>n)) return 6; char *out=fossilize((char*)b,len); hex((unsigned char*)out,strlen(out)); printf(" %zu\n",strlen(out)); free(out);}
 else return 7;
 free(b); return 0;
}
