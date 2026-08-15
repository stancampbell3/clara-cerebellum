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

/* External declarations for ad hoc Coire topic (clara-ritual) FFI callbacks */
extern char* rust_ritual_topic_create(const char* subject_path);
extern char* rust_ritual_topic_list(void);
extern char* rust_ritual_topic_delete(const char* subject_path);
extern char* rust_ritual_topic_publish(const char* subject_path, const char* payload_json, const char* options_json);
extern char* rust_ritual_topic_poll(const char* consumer_id, const char* subject_path);
extern char* rust_ritual_topic_poll_from(const char* subject_path, const char* since_offset);
extern void rust_ritual_free_string(char* s);

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
/* RitualTopicCreateWrapper: (ritual-topic-create "path") */
/* Returns string: "ok" or error JSON                    */
/*********************************************************/
static void RitualTopicCreateWrapper(
  Environment *env,
  UDFContext *context,
  UDFValue *returnValue)
  {
   UDFValue argSubject;

   if (! UDFFirstArgument(context, LEXEME_BITS, &argSubject))
     { returnValue->lexemeValue = CreateString(env, "{\"error\":\"missing subject_path\"}"); return; }

   char* result = rust_ritual_topic_create(argSubject.lexemeValue->contents);
   returnValue->lexemeValue = CreateString(env, result);
   rust_ritual_free_string(result);
  }

/*********************************************************/
/* RitualTopicListWrapper: (ritual-topic-list)            */
/* Returns string: JSON array of subject-path strings     */
/*********************************************************/
static void RitualTopicListWrapper(
  Environment *env,
  UDFContext *context,
  UDFValue *returnValue)
  {
   (void) context;
   char* result = rust_ritual_topic_list();
   returnValue->lexemeValue = CreateString(env, result);
   rust_ritual_free_string(result);
  }

/*********************************************************/
/* RitualTopicDeleteWrapper: (ritual-topic-delete "path") */
/* Returns string: "ok" or error JSON                    */
/*********************************************************/
static void RitualTopicDeleteWrapper(
  Environment *env,
  UDFContext *context,
  UDFValue *returnValue)
  {
   UDFValue argSubject;

   if (! UDFFirstArgument(context, LEXEME_BITS, &argSubject))
     { returnValue->lexemeValue = CreateString(env, "{\"error\":\"missing subject_path\"}"); return; }

   char* result = rust_ritual_topic_delete(argSubject.lexemeValue->contents);
   returnValue->lexemeValue = CreateString(env, result);
   rust_ritual_free_string(result);
  }

/*****************************************************************/
/* RitualTopicPublishWrapper:                                    */
/*   (ritual-topic-publish "path" "{payload}" "{options}"|"")     */
/* Returns string: {"tephra_id":"..."} or error JSON              */
/*****************************************************************/
static void RitualTopicPublishWrapper(
  Environment *env,
  UDFContext *context,
  UDFValue *returnValue)
  {
   UDFValue argSubject, argPayload, argOptions;

   if (! UDFFirstArgument(context, LEXEME_BITS, &argSubject))
     { returnValue->lexemeValue = CreateString(env, "{\"error\":\"missing subject_path\"}"); return; }
   if (! UDFNextArgument(context, LEXEME_BITS, &argPayload))
     { returnValue->lexemeValue = CreateString(env, "{\"error\":\"missing payload_json\"}"); return; }
   if (! UDFNextArgument(context, LEXEME_BITS, &argOptions))
     { returnValue->lexemeValue = CreateString(env, "{\"error\":\"missing options_json\"}"); return; }

   char* result = rust_ritual_topic_publish(
     argSubject.lexemeValue->contents,
     argPayload.lexemeValue->contents,
     argOptions.lexemeValue->contents);

   returnValue->lexemeValue = CreateString(env, result);
   rust_ritual_free_string(result);
  }

/*********************************************************/
/* RitualTopicPollWrapper: (ritual-topic-poll "consumer" "path") */
/* Returns string: JSON array of envelopes                */
/*********************************************************/
static void RitualTopicPollWrapper(
  Environment *env,
  UDFContext *context,
  UDFValue *returnValue)
  {
   UDFValue argConsumer, argSubject;

   if (! UDFFirstArgument(context, LEXEME_BITS, &argConsumer))
     { returnValue->lexemeValue = CreateString(env, "{\"error\":\"missing consumer_id\"}"); return; }
   if (! UDFNextArgument(context, LEXEME_BITS, &argSubject))
     { returnValue->lexemeValue = CreateString(env, "{\"error\":\"missing subject_path\"}"); return; }

   char* result = rust_ritual_topic_poll(
     argConsumer.lexemeValue->contents,
     argSubject.lexemeValue->contents);

   returnValue->lexemeValue = CreateString(env, result);
   rust_ritual_free_string(result);
  }

/*******************************************************************/
/* RitualTopicPollFromWrapper: (ritual-topic-poll-from "path" 0)   */
/* Returns string: {"envelopes":[...],"next_offset":N}             */
/*******************************************************************/
static void RitualTopicPollFromWrapper(
  Environment *env,
  UDFContext *context,
  UDFValue *returnValue)
  {
   UDFValue argSubject, argOffset;
   char offsetBuf[32];

   if (! UDFFirstArgument(context, LEXEME_BITS, &argSubject))
     { returnValue->lexemeValue = CreateString(env, "{\"error\":\"missing subject_path\"}"); return; }
   if (! UDFNextArgument(context, INTEGER_BIT, &argOffset))
     { returnValue->lexemeValue = CreateString(env, "{\"error\":\"missing since_offset\"}"); return; }

   snprintf(offsetBuf, sizeof(offsetBuf), "%lld", (long long) argOffset.integerValue->contents);

   char* result = rust_ritual_topic_poll_from(argSubject.lexemeValue->contents, offsetBuf);
   returnValue->lexemeValue = CreateString(env, result);
   rust_ritual_free_string(result);
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

   /* Register ad hoc Coire topic (clara-ritual) functions */
   AddUDF(env, "ritual-topic-create", "s", 1, 1, "s", RitualTopicCreateWrapper, "RitualTopicCreateWrapper", NULL);
   AddUDF(env, "ritual-topic-list", "s", 0, 0, "", RitualTopicListWrapper, "RitualTopicListWrapper", NULL);
   AddUDF(env, "ritual-topic-delete", "s", 1, 1, "s", RitualTopicDeleteWrapper, "RitualTopicDeleteWrapper", NULL);
   AddUDF(env, "ritual-topic-publish", "s", 3, 3, "s;s;s", RitualTopicPublishWrapper, "RitualTopicPublishWrapper", NULL);
   AddUDF(env, "ritual-topic-poll", "s", 2, 2, "s;s", RitualTopicPollWrapper, "RitualTopicPollWrapper", NULL);
   AddUDF(env, "ritual-topic-poll-from", "s", 2, 2, "s;l", RitualTopicPollFromWrapper, "RitualTopicPollFromWrapper", NULL);
  }
