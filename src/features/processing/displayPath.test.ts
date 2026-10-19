import {describe,expect,it} from 'vitest';
import {displayPath} from './displayPath';

describe('displayPath',()=>{
  it.each([
    [String.raw`\\?\D:\目录\视频.mp4`, String.raw`D:\目录\视频.mp4`],
    [String.raw`\\?\UNC\server\share\视频.mp4`, String.raw`\\server\share\视频.mp4`],
    [String.raw`\\?\unc\server\share\视频.mp4`, String.raw`\\server\share\视频.mp4`],
    [String.raw`\\server\share\视频.mp4`, String.raw`\\server\share\视频.mp4`],
    [String.raw`D:\视频.mp4`, String.raw`D:\视频.mp4`],
    ['/Users/test/视频.mp4', '/Users/test/视频.mp4'],
    [String.raw`/tmp/\\?\video.mp4`, String.raw`/tmp/\\?\video.mp4`],
    [String.raw`\\?\Volume{example}\video.mp4`, String.raw`\\?\Volume{example}\video.mp4`],
    ['', ''],
  ])('formats %s for display without changing unrelated paths',(input,expected)=>{
    expect(displayPath(input)).toBe(expected);
  });
});
