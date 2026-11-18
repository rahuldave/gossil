char *fossilize(const char *zIn, int nIn){
  int n, i, j, c;
  char *zOut;
  if( nIn<0 ) nIn = strlen(zIn);
  for(i=n=0; i<nIn; i++){
    c = zIn[i];
    if( c==0 || c==' ' || c=='\n' || c=='\t' || c=='\r' || c=='\f' || c=='\v'
             || c=='\\' ) n++;
  }
  n += nIn;
  zOut = fossil_malloc( n+1 );
  if( zOut ){
    for(i=j=0; i<nIn; i++){
      int c = zIn[i];
      if( c==0 ){
        zOut[j++] = '\\';
        zOut[j++] = '0';
      }else if( c=='\\' ){
        zOut[j++] = '\\';
        zOut[j++] = '\\';
      }else if( fossil_isspace(c) ){
        zOut[j++] = '\\';
        switch( c ){
          case '\n':  c = 'n'; break;
          case ' ':   c = 's'; break;
          case '\t':  c = 't'; break;
          case '\r':  c = 'r'; break;
          case '\v':  c = 'v'; break;
          case '\f':  c = 'f'; break;
        }
        zOut[j++] = c;
      }else{
        zOut[j++] = c;
      }
    }
    zOut[j] = 0;
  }
  return zOut;
}

void defossilize(char *z){
  int i, j, c;
  char *zSlash = strchr(z, '\\');
  if( zSlash==0 ) return;
  i = zSlash - z;
  for(j=i; (c=z[i])!=0; i++){
    if( c=='\\' && z[i+1] ){
      i++;
      switch( z[i] ){
        case 'n':  c = '\n';  break;
        case 's':  c = ' ';   break;
        case 't':  c = '\t';  break;
        case 'r':  c = '\r';  break;
        case 'v':  c = '\v';  break;
        case 'f':  c = '\f';  break;
        case '0':  c = 0;     break;
        case '\\': c = '\\';  break;
        default:   c = z[i];  break;
      }
    }
    z[j++] = c;
  }
  if( z[j] ) z[j] = 0;
}
