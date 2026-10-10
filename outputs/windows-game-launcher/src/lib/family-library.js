import { gameMetadata } from './game-metadata.js';

export function familyInfo(game) {
  if(game.source!=='steam')return null;
  const info=gameMetadata(game).steamFamily;
  return info&&typeof info==='object'?info:null;
}

export const isFamilyGame=game=>familyInfo(game)?.shared===true;
export const isFamilyUnavailable=game=>isFamilyGame(game)&&familyInfo(game).available===false;
