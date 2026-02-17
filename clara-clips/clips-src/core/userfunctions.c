   /*******************************************************/
   /*      "C" Language Integrated Production System      */
   /*                                                     */
   /*            CLIPS Version 6.40  07/30/16             */
   /*                                                     */
   /*                USER FUNCTIONS MODULE                */
   /*******************************************************/

/*************************************************************/
/* Purpose:                                                  */
/*                                                           */
/* Principal Programmer(s):                                  */
/*      Gary D. Riley                                        */
/*                                                           */
/* Contributing Programmer(s):                               */
/*                                                           */
/* Revision History:                                         */
/*                                                           */
/*      6.24: Created file to seperate UserFunctions and     */
/*            EnvUserFunctions from main.c.                  */
/*                                                           */
/*      6.30: Removed conditional code for unsupported       */
/*            compilers/operating systems (IBM_MCW,          */
/*            MAC_MCW, and IBM_TBC).                         */
/*                                                           */
/*            Removed use of void pointers for specific      */
/*            data structures.                               */
/*                                                           */
/*************************************************************/

/***************************************************************************/
/*                                                                         */
/* Permission is hereby granted, free of charge, to any person obtaining   */
/* a copy of this software and associated documentation files (the         */
/* "Software"), to deal in the Software without restriction, including     */
/* without limitation the rights to use, copy, modify, merge, publish,     */
/* distribute, and/or sell copies of the Software, and to permit persons   */
/* to whom the Software is furnished to do so.                             */
/*                                                                         */
/* THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS */
/* OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF              */
/* MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT   */
/* OF THIRD PARTY RIGHTS. IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY  */
/* CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES, OR ANY DAMAGES */
/* WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN   */
/* ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF */
/* OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.          */
/*                                                                         */
/***************************************************************************/

#include "clips.h"

void UserFunctions(Environment *);

/* External declarations for Rust FFI callbacks */
extern char* rust_clara_evaluate(void* env, const char* input);
extern void rust_free_string(char* s);

/* External declarations for Coire FFI callbacks */
extern char* rust_coire_emit(const char* session, const char* origin, const char* payload);
extern char* rust_coire_poll(const char* session);
extern char* rust_coire_mark(const char* event_id);
extern long long rust_coire_count(const char* session);
extern void rust_coire_free_string(char* s);

/*********************************************************/
/* ClaraEvaluateWrapper: C wrapper for Rust callback     */
/* This function is registered with CLIPS and calls the  */
/* Rust implementation of clara-evaluate                 */
/*********************************************************/
static void ClaraEvaluateWrapper(
  Environment *env,
  UDFContext *context,
  UDFValue *returnValue)
  {
   UDFValue arg;

   /* Get the first argument (JSON string) */
   if (! UDFFirstArgument(context, LEXEME_BITS, &arg))
     {
      returnValue->lexemeValue = CreateString(env, "{\"status\":\"error\",\"message\":\"Invalid argument\"}");
      return;
     }

   const char* input = arg.lexemeValue->contents;

   /* Call Rust callback */
   char* result = rust_clara_evaluate((void*)env, input);

   /* Create CLIPS string from result */
   returnValue->lexemeValue = CreateString(env, result);

   /* Free the Rust-allocated string */
   rust_free_string(result);
  }

/*********************************************************/
/* CoireEmitWrapper: (coire-emit "session" "origin" "{}") */
/* Returns string: "ok" or error JSON                    */
/*********************************************************/
static void CoireEmitWrapper(
  Environment *env,
  UDFContext *context,
  UDFValue *returnValue)
  {
   UDFValue argSession, argOrigin, argPayload;

   if (! UDFFirstArgument(context, LEXEME_BITS, &argSession))
     { returnValue->lexemeValue = CreateString(env, "{\"error\":\"missing session_id\"}"); return; }
   if (! UDFNextArgument(context, LEXEME_BITS, &argOrigin))
     { returnValue->lexemeValue = CreateString(env, "{\"error\":\"missing origin\"}"); return; }
   if (! UDFNextArgument(context, LEXEME_BITS, &argPayload))
     { returnValue->lexemeValue = CreateString(env, "{\"error\":\"missing payload\"}"); return; }

   char* result = rust_coire_emit(
     argSession.lexemeValue->contents,
     argOrigin.lexemeValue->contents,
     argPayload.lexemeValue->contents);

   returnValue->lexemeValue = CreateString(env, result);
   rust_coire_free_string(result);
  }

/*********************************************************/
/* CoirePollWrapper: (coire-poll "session")              */
/* Returns string: JSON array of events                  */
/*********************************************************/
static void CoirePollWrapper(
  Environment *env,
  UDFContext *context,
  UDFValue *returnValue)
  {
   UDFValue argSession;

   if (! UDFFirstArgument(context, LEXEME_BITS, &argSession))
     { returnValue->lexemeValue = CreateString(env, "{\"error\":\"missing session_id\"}"); return; }

   char* result = rust_coire_poll(argSession.lexemeValue->contents);
   returnValue->lexemeValue = CreateString(env, result);
   rust_coire_free_string(result);
  }

/*********************************************************/
/* CoireMarkWrapper: (coire-mark "event-uuid")           */
/* Returns string: "ok" or error JSON                    */
/*********************************************************/
static void CoireMarkWrapper(
  Environment *env,
  UDFContext *context,
  UDFValue *returnValue)
  {
   UDFValue argEventId;

   if (! UDFFirstArgument(context, LEXEME_BITS, &argEventId))
     { returnValue->lexemeValue = CreateString(env, "{\"error\":\"missing event_id\"}"); return; }

   char* result = rust_coire_mark(argEventId.lexemeValue->contents);
   returnValue->lexemeValue = CreateString(env, result);
   rust_coire_free_string(result);
  }

/*********************************************************/
/* CoireCountWrapper: (coire-count "session")            */
/* Returns integer: count of pending events              */
/*********************************************************/
static void CoireCountWrapper(
  Environment *env,
  UDFContext *context,
  UDFValue *returnValue)
  {
   UDFValue argSession;

   if (! UDFFirstArgument(context, LEXEME_BITS, &argSession))
     { returnValue->integerValue = CreateInteger(env, -1); return; }

   long long count = rust_coire_count(argSession.lexemeValue->contents);
   returnValue->integerValue = CreateInteger(env, count);
  }

/*********************************************************/
/* UserFunctions: Informs the expert system environment  */
/*   of any user defined functions. In the default case, */
/*   there are no user defined functions. To define      */
/*   functions, either this function must be replaced by */
/*   a function with the same name within this file, or  */
/*   this function can be deleted from this file and     */
/*   included in another file.                           */
/*********************************************************/
void UserFunctions(
  Environment *env)
  {
   /* Register clara-evaluate function */
   /* Signature: "s" = returns string, 1,1 = min/max args, "s" = arg must be string */
   AddUDF(env, "clara-evaluate", "s", 1, 1, "s", ClaraEvaluateWrapper, "ClaraEvaluateWrapper", NULL);

   /* Register coire functions */
   AddUDF(env, "coire-emit", "s", 3, 3, "s;s;s", CoireEmitWrapper, "CoireEmitWrapper", NULL);
   AddUDF(env, "coire-poll", "s", 1, 1, "s", CoirePollWrapper, "CoirePollWrapper", NULL);
   AddUDF(env, "coire-mark", "s", 1, 1, "s", CoireMarkWrapper, "CoireMarkWrapper", NULL);
   AddUDF(env, "coire-count", "l", 1, 1, "s", CoireCountWrapper, "CoireCountWrapper", NULL);
  }
