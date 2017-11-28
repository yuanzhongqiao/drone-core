use failure::{err_msg, Error};
use proc_macro::TokenStream;
use quote::Tokens;
use syn::{parse_token_trees, DelimToken, Delimited, Ident, Token, TokenTree};

pub(crate) fn thread_local(input: TokenStream) -> Result<Tokens, Error> {
  let input = parse_token_trees(&input.to_string()).map_err(err_msg)?;
  let mut input = input.into_iter();
  let mut attributes = Vec::new();
  let mut field_visiblity = Vec::new();
  let mut field_attributes = Vec::new();
  let mut field_name = Vec::new();
  let mut field_type = Vec::new();
  let mut field_init = Vec::new();
  'outer: loop {
    let mut public = false;
    let mut inner_attributes = Vec::new();
    loop {
      match input.next() {
        Some(TokenTree::Token(Token::DocComment(ref string)))
          if string.starts_with("//!") =>
        {
          let string = string.trim_left_matches("//!");
          attributes.push(quote!(#[doc = #string]));
        }
        Some(TokenTree::Token(Token::DocComment(ref string)))
          if string.starts_with("///") =>
        {
          let string = string.trim_left_matches("///");
          inner_attributes.push(quote!(#[doc = #string]));
        }
        Some(TokenTree::Token(Token::Pound)) => match input.next() {
          Some(TokenTree::Token(Token::Not)) => match input.next() {
            Some(TokenTree::Delimited(delimited)) => {
              attributes.push(quote!(# #delimited))
            }
            token => {
              Err(format_err!("Invalid tokens after `#!`: {:?}", token))?
            }
          },
          Some(TokenTree::Delimited(delimited)) => {
            inner_attributes.push(quote!(# #delimited))
          }
          token => Err(format_err!("Invalid tokens after `#`: {:?}", token))?,
        },
        Some(TokenTree::Token(Token::Ident(ref ident))) if ident == "pub" => {
          public = true;
        }
        Some(TokenTree::Token(Token::Ident(name))) => {
          match input.next() {
            Some(TokenTree::Token(Token::Colon)) => (),
            token => {
              Err(format_err!("Invalid token after `{}`: {:?}", name, token))?
            }
          }
          let mut ty = Vec::new();
          loop {
            match input.next() {
              Some(TokenTree::Token(Token::Eq)) => break,
              Some(TokenTree::Token(token)) => ty.push(token),
              token => Err(format_err!(
                "Invalid token after `{}: {:?}`: {:?}",
                name,
                ty,
                token
              ))?,
            }
          }
          let init = match input.next() {
            Some(TokenTree::Delimited(Delimited {
              delim: DelimToken::Brace,
              tts,
            })) => tts,
            token => Err(format_err!(
              "Invalid token after `{}: {:?} =`: {:?}",
              name,
              ty,
              token
            ))?,
          };
          field_visiblity.push(if public {
            Some(Ident::new("pub"))
          } else {
            None
          });
          field_attributes.push(inner_attributes);
          field_name.push(name);
          field_type.push(ty);
          field_init.push(init);
          break;
        }
        None => break 'outer,
        token => Err(format_err!("Invalid token: {:?}", token))?,
      }
    }
  }
  let field_name2 = field_name.clone();

  Ok(quote! {
    use ::drone::thread::{self, Chain, TaskCell};

    #(#attributes)*
    pub struct ThreadLocal {
      chain: Chain,
      task: TaskCell,
      preempted_id: usize,
      #(
        #(#field_attributes)*
        #field_visiblity #field_name: #(#field_type)*,
      )*
    }

    impl ThreadLocal {
      /// Creates a blank `ThreadLocal`.
      #[allow(dead_code)]
      #[inline(always)]
      pub const fn new(_id: usize) -> Self {
        Self {
          chain: Chain::new(),
          task: TaskCell::new(),
          preempted_id: 0,
          #(
            #field_name2: { #(#field_init)* },
          )*
        }
      }
    }

    impl Thread for ThreadLocal {
      #[inline(always)]
      unsafe fn get_unchecked(id: usize) -> &'static Self {
        THREADS.get_unchecked(id)
      }

      #[inline(always)]
      fn chain(&self) -> &Chain {
        &self.chain
      }

      #[inline(always)]
      fn chain_mut(&mut self) -> &mut Chain {
        &mut self.chain
      }

      #[inline(always)]
      fn task(&self) -> &TaskCell {
        &self.task
      }

      #[inline(always)]
      fn preempted_id(&self) -> usize {
        self.preempted_id
      }

      #[inline(always)]
      fn set_preempted_id(&mut self, id: usize) {
        self.preempted_id = id;
      }
    }

    /// Initialize the `futures` task system.
    ///
    /// See [`thread::init()`] for more details.
    ///
    /// [`thread::init()`]: ../../drone/thread/fn.init.html
    #[inline(always)]
    pub unsafe fn init() {
      thread::init::<ThreadLocal>();
    }
  })
}
