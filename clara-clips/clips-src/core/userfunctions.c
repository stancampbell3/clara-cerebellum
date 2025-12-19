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
  }
