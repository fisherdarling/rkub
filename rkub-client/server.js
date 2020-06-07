var express = require('express');
var app = express();

app.use(express.static('deploy')); 
app.use(express.static('pkg')); 

var server = app.listen(8080);