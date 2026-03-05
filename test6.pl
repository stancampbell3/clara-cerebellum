:- dynamic(student/1).
:- dynamic(like/2).

student(jemina).
student(davey).
student(rich).
student(donna).
student(becky).

like(davey, jemina).
like(davey, rich).
like(davey, donna).
like(davey, becky).

like(jemina, davey).
like(jemina, rich).
like(jemina, donna).
like(jemina, becky).

like(rich, davey).
like(rich, jemina).
like(rich, donna).
like(rich, becky).

like(donna, davey).

% donna likes daveys friends
test1(A) :- like(donna, davey), like(davey, A), assert(like(donna, A)).


